//! Wrapper for DPDK's environment abstraction layer (EAL).
use ffi;
use log::{debug, info, warn};
use std::collections::{HashMap, HashSet};
use std::convert::{TryFrom, TryInto};
use std::ffi::CString;
use std::ptr::NonNull;
use std::sync::{Arc, Mutex};
use thiserror::Error;

const MAGIC: &str = "be0dd4ab";

const DEFAULT_TX_DESC: u16 = 128;
const DEFAULT_RX_DESC: u16 = 128;
const DEFAULT_RX_POOL_SIZE: usize = 1024;
const DEFAULT_RX_PER_CORE_CACHE: usize = 0;
const DEFAULT_MBUF_PRIV_SIZE: usize = 0;
const DEFAULT_PACKET_DATA_LENGTH: usize = 2048;
const DEFAULT_PROMISC: bool = true;

/// Shared mutating states that all `Eal` instances share.
#[derive(Debug)]
struct EalGlobalInner {
    // Whether `setup` has been successfully invoked.
    setup_initialized: bool,
} // TODO Remove this if unnecessary

impl Default for EalGlobalInner {
    #[inline]
    fn default() -> Self {
        Self {
            setup_initialized: false,
        }
    }
}

#[derive(Debug)]
struct EalInner {
    shared: Mutex<EalGlobalInner>,
}

/// DPDK's environment abstraction layer (EAL).
///
/// This object indicates that EAL has been initialized and its APIs are available now.
#[derive(Debug, Clone)]
pub struct Eal {
    inner: Arc<EalInner>,
}

#[derive(Debug, Error)]
pub enum EalError {
    #[error("EAL function returned an error code: {}", code)]
    ErrorCode { code: i32 },
}

/// How to create NIC queues for a CPU.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum Affinity {
    /// All NICs create queues for the CPU.
    Full,
    /// NICs on the same NUMA node create queues for the CPU.
    Numa,
}

/// Abstract type for DPDK port
#[derive(Debug, Clone)]
pub struct Port {
    inner: Arc<PortInner>,
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct LCoreId(u32);

impl Into<u32> for LCoreId {
    #[inline]
    fn into(self) -> u32 {
        self.0
    }
}

impl LCoreId {
    #[inline]
    fn new(id: u32) -> Self {
        Self(id)
    }
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct SocketId(u32);

impl Into<u32> for SocketId {
    #[inline]
    fn into(self) -> u32 {
        self.0
    }
}

impl SocketId {
    #[inline]
    fn new(id: u32) -> Self {
        Self(id)
    }
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct ErrorCode(u8);

impl Into<u8> for ErrorCode {
    #[inline]
    fn into(self) -> u8 {
        self.0
    }
}

impl From<u8> for ErrorCode {
    #[inline]
    fn from(id: u8) -> Self {
        Self(id)
    }
}
impl TryFrom<u32> for ErrorCode {
    type Error = <u8 as TryFrom<u32>>::Error;
    #[inline]
    fn try_from(id: u32) -> Result<Self, Self::Error> {
        Ok(Self(id.try_into()?))
    }
}
impl TryFrom<i32> for ErrorCode {
    type Error = <u8 as TryFrom<i32>>::Error;
    #[inline]
    fn try_from(id: i32) -> Result<Self, Self::Error> {
        Ok(Self((-id).try_into()?))
    }
}

impl Port {
    /// Returns NUMA node of current port.
    #[inline]
    pub fn socket_id(&self) -> SocketId {
        SocketId::new(unsafe {
            dpdk_sys::rte_eth_dev_socket_id(self.inner.port_id)
                .try_into()
                .unwrap()
        })
    }
}

#[derive(Debug)]
struct PortInner {
    port_id: u16,
    eal: Arc<EalInner>,
}

/// Abstract type for DPDK MPool
#[derive(Debug, Clone)]
pub struct MPool {
    inner: Arc<MPoolInner>,
}

#[derive(Debug)]
struct MPoolInner {
    ptr: NonNull<dpdk_sys::rte_mempool>,
    eal: Arc<EalInner>,
}

/// # Safety
/// Mempools are thread-safe.
/// https://doc.dpdk.org/guides/prog_guide/thread_safety_dpdk_functions.html
unsafe impl Send for MPoolInner {}
unsafe impl Sync for MPoolInner {}

impl Drop for MPoolInner {
    #[inline]
    fn drop(&mut self) {
        // Safety: foreign function.
        unsafe { dpdk_sys::rte_mempool_free(self.ptr.as_ptr()) };
    }
}

impl MPool {
    /// Create a new `MPool`.  Note: Pool name must be globally unique.
    ///
    /// @param n The number of elements in the mbuf pool.
    ///
    /// @param cache_size Size of the per-core object cache.
    ///
    /// @param priv_size Size of application private are between the rte_mbuf structure and the data
    /// buffer. This value must be aligned to RTE_MBUF_PRIV_ALIGN.
    ///
    /// @param data_room_size Size of data buffer in each mbuf, including RTE_PKTMBUF_HEADROOM.
    ///
    /// @param socket_id The socket identifier where the memory should be allocated. The value can
    /// be `None` (corresponds to DPDK's *SOCKET_ID_ANY*) if there is no NUMA constraint for the reserved zone.
    #[inline]
    pub fn new<S: AsRef<str>>(
        eal: &Eal,
        name: S,
        n: usize,
        cache_size: usize,
        priv_size: usize,
        data_room_size: usize,
        socket_id: Option<SocketId>,
    ) -> Self {
        let pool_name = CString::new(name.as_ref()).unwrap();

        // Safety: foreign function.
        // Note: if we need additional metadata (priv_data),
        // assign `(((size_of::<MPoolPriv>() + 7) / 8) * 8) as u16` instead of `0`.
        let ptr = unsafe {
            dpdk_sys::rte_pktmbuf_pool_create(
                pool_name.into_bytes_with_nul().as_ptr() as *mut i8,
                n.try_into().unwrap(),
                cache_size as u32,
                priv_size.try_into().unwrap(),
                data_room_size.try_into().unwrap(),
                socket_id
                    .map(|x| x.0 as i32)
                    .unwrap_or(dpdk_sys::SOCKET_ID_ANY),
            )
        };
        // The pointer to the new allocated mempool, on success. NULL on error with rte_errno set appropriately.
        // https://doc.dpdk.org/api/rte__mbuf_8h.html
        MPool {
            inner: Arc::new(MPoolInner {
                ptr: NonNull::new(ptr).unwrap(),
                eal: eal.inner.clone(),
            }),
        }
    }

    /// Allocate a `Packet` from the pool.
    ///
    /// # Safety
    ///
    /// Returned item must not outlive this pool.
    #[inline]
    pub unsafe fn alloc(&self) -> Option<Packet> {
        // Safety: foreign function.
        // `alloc` is temporarily unsafe. Leaving this unsafe block.
        let pkt_ptr = unsafe { dpdk_sys::rte_pktmbuf_alloc(self.inner.ptr.as_ptr()) };

        Some(Packet {
            ptr: NonNull::new(pkt_ptr)?,
        })
    }
}

#[derive(Debug)]
pub struct Packet {
    ptr: NonNull<dpdk_sys::rte_mbuf>,
}

impl Drop for Packet {
    #[inline]
    fn drop(&mut self) {
        // Safety: foreign function.
        unsafe {
            dpdk_sys::rte_pktmbuf_free(self.ptr.as_ptr());
        }
    }
}

/// Abstract type for DPDK RxQ
///
/// TODO Support per-queue RX operations
#[derive(Debug, Clone)]
pub struct RxQ {
    inner: Arc<RxQInner>,
}

/// Note: RxQ requires a dedicated mempool to receive incoming packets.
#[derive(Debug)]
struct RxQInner {
    queue_id: u16,
    port: Arc<PortInner>,
    mpool: Arc<MPoolInner>,
}

impl Drop for RxQInner {
    #[inline]
    fn drop(&mut self) {
        // Safety: foreign function.
        //
        // Note: dynamically starting/stopping queue may not be supported by the driver.
        let ret = unsafe { dpdk_sys::rte_eth_dev_rx_queue_stop(self.port.port_id, self.queue_id) };
        if ret != 0 {
            warn!(
                "RxQInner::drop, non-severe error code({}) while stopping queue {}:{}",
                ret, self.port.port_id, self.queue_id
            );
        }
    }
}

impl RxQ {}

/// Abstract type for DPDK TxQ
#[derive(Debug, Clone)]
pub struct TxQ {
    inner: Arc<TxQInner>,
}

/// Note: while RxQ requires a dedicated mempool, Tx operation takes `MBuf`s which are allocated by
/// other RxQ's mempool or other externally allocated mempools. Thus, TxQ itself does not require
/// its own mempool.
#[derive(Debug)]
struct TxQInner {
    queue_id: u16,
    port: Arc<PortInner>,
}

impl Drop for TxQInner {
    #[inline]
    fn drop(&mut self) {
        // Safety: foreign function.
        //
        // Note: dynamically starting/stopping queue may not be supported by the driver.
        let ret = unsafe { dpdk_sys::rte_eth_dev_tx_queue_stop(self.port.port_id, self.queue_id) };
        if ret != 0 {
            warn!(
                "TxQInner::drop, non-severe error code({}) while stopping queue {}:{}",
                ret, self.port.port_id, self.queue_id
            );
        }
    }
}

impl TxQ {}

impl Eal {
    /// Create an `Eal` instance.
    ///
    /// It takes command-line arguments and consumes used arguments.
    #[inline]
    pub fn new(args: &mut Vec<String>) -> Result<Self, EalError> {
        Ok(Eal {
            inner: Arc::new(EalInner::new(args)?),
        })
    }

    /// Setup per-core Rx queues and Tx queues according to the given affinity.  Currently, this
    /// must be called once for the whole program. Otherwise it will return an error code.  Returns
    /// array of `(logical core id, assigned rx queues, assigned tx queues)` on success.
    ///
    /// Note: rte_lcore_count: -c ff 옵션에 따라 줄어듬.
    /// Note: we have clippy warning: complex return type.
    #[inline]
    pub fn setup(
        &self,
        rx_affinity: Affinity,
        tx_affinity: Affinity,
    ) -> Result<Vec<(LCoreId, Vec<RxQ>, Vec<TxQ>)>, ErrorCode> {
        // Acquire globally shared state and check whether already initialized.
        let mut shared_mut = self.inner.shared.lock().unwrap();
        if shared_mut.setup_initialized {
            // Already initialized.
            return Err(dpdk_sys::EALREADY.try_into().unwrap());
        }

        // List of valid logical core ids.
        // Note: If some cores are masked, range (0..rte_lcore_count()) will include disabled cores.
        let lcore_id_list = (0..dpdk_sys::RTE_MAX_LCORE)
            .filter(|index| unsafe { dpdk_sys::rte_lcore_is_enabled(*index) > 0 })
            .collect::<Vec<_>>();

        // Map of `socket_id` to set of `lcore_id`s belong to the socket.
        let mut socket_to_lcore_map = HashMap::new();
        for lcore_id in &lcore_id_list {
            let lcore_id = *lcore_id;
            // Safety: foreign function.
            let socket_id =
                unsafe { dpdk_sys::rte_lcore_to_socket_id(lcore_id.try_into().unwrap()) };
            // Safety: foreign function.
            let cpu_id = unsafe { dpdk_sys::rte_lcore_to_cpu_id(lcore_id.try_into().unwrap()) };
            debug!(
                "Logical core {} is enabled at physical core {} (NUMA node {})",
                lcore_id, cpu_id, socket_id
            );

            // Classify `lcore_id`s according to their socket IDs.
            socket_to_lcore_map
                .entry(SocketId::new(socket_id))
                .or_insert_with(HashSet::new)
                .insert(LCoreId::new(lcore_id));
        }
        debug!("lcore count: {}", socket_to_lcore_map.len());

        // List of `Port`s.
        let port_list = (0..u16::try_from(dpdk_sys::RTE_MAX_ETHPORTS).unwrap())
            .filter(|index| {
                // Safety: foreign function.
                unsafe { dpdk_sys::rte_eth_dev_is_valid_port(*index) > 0 }
            })
            .map(|port_id| Port {
                inner: Arc::new(PortInner {
                    port_id,
                    eal: self.inner.clone(),
                }),
            })
            .collect::<Vec<_>>();

        // Param: `Port`, `Vec<LcoreId>`, `Affinity` for Rx and Tx.
        // Returns `(Vec<rx_lcore_ids>, Vec<tx_lcore_ids>)` assigned to the given `Port`.
        fn extract_rxtx_lcores(
            port: &Port,
            socket_to_lcore_map: &HashMap<SocketId, HashSet<LCoreId>>,
            rx_affinity: Affinity,
            tx_affinity: Affinity,
        ) -> (HashSet<LCoreId>, HashSet<LCoreId>) {
            let socket_id = port.socket_id();
            let rx_cpus = match rx_affinity {
                Affinity::Full => socket_to_lcore_map.values().flatten().cloned().collect(),
                Affinity::Numa => socket_to_lcore_map.get(&socket_id).unwrap().clone(),
            };
            let tx_cpus = match tx_affinity {
                Affinity::Full => socket_to_lcore_map.values().flatten().cloned().collect(),
                Affinity::Numa => socket_to_lcore_map.get(&socket_id).unwrap().clone(),
            };
            (rx_cpus, tx_cpus)
        }

        // Init each port
        fn configure_port(
            port: &Port,
            num_rxq: usize,
            num_txq: usize,
        ) -> dpdk_sys::rte_eth_dev_info {
            let port_id = port.inner.port_id;
            // Safety: `rte_eth_dev_info` contains primitive integer types. Safe to fill with zeros.
            let mut dev_info: dpdk_sys::rte_eth_dev_info = unsafe { std::mem::zeroed() };
            // Safety: foreign function.
            unsafe { dpdk_sys::rte_eth_dev_info_get(port_id, &mut dev_info) };

            let rx_queue_limit = dev_info.max_rx_queues;
            let tx_queue_limit = dev_info.max_tx_queues;
            let rx_queue_count: u16 = num_rxq.try_into().unwrap();
            let tx_queue_count: u16 = num_txq.try_into().unwrap();

            assert!(rx_queue_count <= rx_queue_limit);
            assert!(tx_queue_count <= tx_queue_limit);

            assert!(DEFAULT_RX_DESC <= dev_info.rx_desc_lim.nb_max);
            assert!(DEFAULT_RX_DESC >= dev_info.rx_desc_lim.nb_min);
            assert!(DEFAULT_RX_DESC % dev_info.rx_desc_lim.nb_align == 0);

            assert!(DEFAULT_TX_DESC <= dev_info.tx_desc_lim.nb_max);
            assert!(DEFAULT_TX_DESC >= dev_info.tx_desc_lim.nb_min);
            assert!(DEFAULT_TX_DESC % dev_info.tx_desc_lim.nb_align == 0);

            // Safety: `rte_eth_conf` contains primitive integer types. Safe to fill with zeros.
            let mut port_conf: dpdk_sys::rte_eth_conf = unsafe { std::mem::zeroed() };
            port_conf.rxmode.max_rx_pkt_len = dpdk_sys::RTE_ETHER_MAX_LEN;
            port_conf.rxmode.mq_mode = dpdk_sys::rte_eth_rx_mq_mode_ETH_MQ_RX_NONE;
            port_conf.txmode.mq_mode = dpdk_sys::rte_eth_tx_mq_mode_ETH_MQ_TX_NONE;

            if rx_queue_count > 1 {
                port_conf.rxmode.mq_mode = dpdk_sys::rte_eth_rx_mq_mode_ETH_MQ_RX_RSS;
                port_conf.rx_adv_conf.rss_conf.rss_hf = (dpdk_sys::ETH_RSS_NONFRAG_IPV4_UDP
                    | dpdk_sys::ETH_RSS_NONFRAG_IPV4_TCP)
                    .into();
                // TODO set symmetric RSS for TCP/IP
            }

            // Enable offload flags
            if dev_info.rx_offload_capa & u64::from(dpdk_sys::DEV_RX_OFFLOAD_CHECKSUM) > 0 {
                info!("RX CKSUM Offloading is on for port {}", port_id);
                port_conf.rxmode.offloads |= u64::from(dpdk_sys::DEV_RX_OFFLOAD_CHECKSUM);
            }
            if dev_info.tx_offload_capa & u64::from(dpdk_sys::DEV_TX_OFFLOAD_IPV4_CKSUM) > 0 {
                info!("TX IPv4 CKSUM Offloading is on for port {}", port_id);
                port_conf.txmode.offloads |= u64::from(dpdk_sys::DEV_TX_OFFLOAD_IPV4_CKSUM);
            }
            if dev_info.tx_offload_capa & u64::from(dpdk_sys::DEV_TX_OFFLOAD_UDP_CKSUM) > 0 {
                info!("TX UDP CKSUM Offloading is on for port {}", port_id);
                port_conf.txmode.offloads |= u64::from(dpdk_sys::DEV_TX_OFFLOAD_UDP_CKSUM);
            }
            if dev_info.tx_offload_capa & u64::from(dpdk_sys::DEV_TX_OFFLOAD_TCP_CKSUM) > 0 {
                info!("TX TCP CKSUM Offloading is on for port {}", port_id);
                port_conf.txmode.offloads |= u64::from(dpdk_sys::DEV_TX_OFFLOAD_TCP_CKSUM);
            }

            // Configure ports
            let ret = unsafe {
                dpdk_sys::rte_eth_dev_configure(port_id, rx_queue_count, tx_queue_count, &port_conf)
            };
            assert_eq!(ret, 0);
            dev_info
        }

        // Configure a Rx queue for the given port and queue index.
        fn configure_rxq(
            eal: &Eal,
            port: &Port,
            dev_info: &dpdk_sys::rte_eth_dev_info,
            rxq_idx: usize,
        ) -> RxQ {
            let port_id = port.inner.port_id;
            // Create MPool for RX
            let pool_name = format!("rxq_{}_{}_{}", MAGIC, port_id, rxq_idx);
            let mpool = MPool::new(
                eal,
                pool_name,
                DEFAULT_RX_POOL_SIZE,
                DEFAULT_MBUF_PRIV_SIZE,
                DEFAULT_RX_PER_CORE_CACHE,
                DEFAULT_PACKET_DATA_LENGTH,
                Some(port.socket_id()),
            );
            let ret = unsafe {
                dpdk_sys::rte_eth_rx_queue_setup(
                    port_id,
                    rxq_idx as u16,
                    DEFAULT_RX_DESC,
                    port.socket_id().into(),
                    &dev_info.default_rxconf,
                    mpool.inner.ptr.as_ptr(),
                )
            };
            assert_eq!(ret, 0);
            RxQ {
                inner: Arc::new(RxQInner {
                    queue_id: rxq_idx as u16,
                    port: port.inner.clone(),
                    mpool: mpool.inner,
                }),
            }
        }

        // Configure a Tx queue for the given port and queue index.
        fn configure_txq(
            port: &Port,
            dev_info: &dpdk_sys::rte_eth_dev_info,
            txq_idx: usize,
        ) -> TxQ {
            let port_id = port.inner.port_id;
            let ret = unsafe {
                dpdk_sys::rte_eth_tx_queue_setup(
                    port_id,
                    txq_idx as u16,
                    DEFAULT_RX_DESC,
                    port.socket_id().into(),
                    &dev_info.default_txconf,
                )
            };
            assert_eq!(ret, 0);

            TxQ {
                inner: Arc::new(TxQInner {
                    queue_id: txq_idx as u16,
                    port: port.inner.clone(),
                }),
            }
        }

        fn start_port(port: &Port) {
            let port_id = port.inner.port_id;
            // Set promisc.
            // Safety: foreign function.
            unsafe {
                if DEFAULT_PROMISC {
                    dpdk_sys::rte_eth_promiscuous_enable(port_id);
                } else {
                    dpdk_sys::rte_eth_promiscuous_disable(port_id);
                }
            };

            // Start port.
            // Safety: foreign function.
            let ret = unsafe { dpdk_sys::rte_eth_dev_start(port_id) };
            assert_eq!(ret, 0);
        }

        // Map from `lcore_id` to its assigned `(rxq, txq)`.
        let mut lcore_to_rxqtxq_map = HashMap::new();
        for port in port_list {
            let (rx_lcores, tx_lcores) =
                extract_rxtx_lcores(&port, &socket_to_lcore_map, rx_affinity, tx_affinity);
            let dev_info = configure_port(&port, rx_lcores.len(), tx_lcores.len());

            for (rxq_idx, rx_lcore) in rx_lcores.into_iter().enumerate() {
                let rxq = configure_rxq(self, &port, &dev_info, rxq_idx);
                lcore_to_rxqtxq_map
                    .entry(rx_lcore)
                    .or_insert_with(|| (Vec::new(), Vec::new()))
                    .0
                    .push(rxq);
            }
            for (txq_idx, tx_lcore) in tx_lcores.into_iter().enumerate() {
                let txq = configure_txq(&port, &dev_info, txq_idx);
                lcore_to_rxqtxq_map
                    .entry(tx_lcore)
                    .or_insert_with(|| (Vec::new(), Vec::new()))
                    .1
                    .push(txq);
            }
            start_port(&port);
        }

        // Initialization finished
        shared_mut.setup_initialized = true;

        // Return array of `(LCore, Vec<RxQ>, Vec<TxQ>)`.
        Ok(lcore_to_rxqtxq_map
            .into_iter()
            .map(|(lcore_id, (rxqs, txqs))| (lcore_id, rxqs, txqs))
            .collect())
    }
}

pub use super::dpdk_sys::EalStaticFunctions as EalGlobalApi;

unsafe impl EalGlobalApi for Eal {}

impl EalInner {
    // Create `EalInner`.
    #[inline]
    fn new(args: &mut Vec<String>) -> Result<Self, EalError> {
        // To prevent DPDK PMDs' being unlinked, we explicitly create symbolic dependency via
        // calling `load_drivers`.
        dpdk_sys::load_drivers();

        // DPDK returns number of consumed argc
        // Safety: foriegn function (safe unless there is a bug)
        let ret = unsafe { ffi::run_with_args(dpdk_sys::rte_eal_init, &*args) };
        if ret < 0 {
            return Err(EalError::ErrorCode { code: ret });
        }

        // Strip first n args and return the remaining
        args.drain(..ret as usize);
        Ok(EalInner {
            shared: Mutex::new(Default::default()),
        })
    }
}

impl Drop for EalInner {
    #[inline]
    fn drop(&mut self) {
        // Safety: foriegn function (safe unless there is a bug)
        unsafe {
            let ret = dpdk_sys::rte_eal_cleanup();
            if ret == -(dpdk_sys::ENOTSUP as i32) {
                warn!("EAL Cleanup is not implemented.");
                return;
            }
            assert_eq!(ret, 0);
        }
    }
}
