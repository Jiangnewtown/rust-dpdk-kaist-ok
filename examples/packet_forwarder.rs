use rust_dpdk::*;
use std::ffi::CString;
use std::os::raw::{c_char, c_int, c_void};
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use std::collections::HashMap;
use std::sync::Mutex;
use rand::Rng;
use rand::rngs::ThreadRng;

// 全局变量，用于控制程序退出
static mut FORCE_QUIT: AtomicBool = AtomicBool::new(false);

// 全局变量，用于跟踪已发送的数据包
static mut PACKET_TRACKER: Option<Arc<Mutex<HashMap<u32, String>>>> = None;

// 安全处理信号
extern "C" fn signal_handler(_signum: c_int) {
    println!("\n收到中断信号，正在安全退出...");
    unsafe {
        FORCE_QUIT.store(true, Ordering::SeqCst);
    }
}

// 数据包转发逻辑
fn process_packet(mbuf: *mut rte_mbuf) {
    unsafe {
        // 获取以太网头部
        // 手动实现 rte_pktmbuf_mtod 宏的功能
        // 原始宏定义: #define rte_pktmbuf_mtod(m, t) ((t)((char *)(m)->buf_addr + (m)->data_off))
        let eth_hdr_ptr = ((*mbuf).buf_addr as *mut u8).add((*mbuf).data_off as usize);
        
        // 以太网头部结构:
        // struct rte_ether_hdr {
        //     struct rte_ether_addr d_addr; /**< Destination address. */
        //     struct rte_ether_addr s_addr; /**< Source address. */
        //     uint16_t ether_type;          /**< Frame type. */
        // }
        
        // 交换源和目标 MAC 地址
        // 每个 MAC 地址是 6 字节
        for i in 0..RTE_ETHER_ADDR_LEN as usize {
            let tmp = *eth_hdr_ptr.add(i);
            *eth_hdr_ptr.add(i) = *eth_hdr_ptr.add(i + RTE_ETHER_ADDR_LEN as usize);
            *eth_hdr_ptr.add(i + RTE_ETHER_ADDR_LEN as usize) = tmp;
        }
        
        // 获取以太网类型 (大端序)
        // 以太网类型在以太网头部偏移 12 字节的位置
        let ether_type_ptr = eth_hdr_ptr.add(12) as *const u16;
        let ether_type = *ether_type_ptr;
        
        // 检查是否是 IPv4 数据包 (0x0800, 但在内存中是 0x0008 因为是大端序)
        let ether_type_ipv4 = ((RTE_ETHER_TYPE_IPV4 as u16) >> 8) | ((RTE_ETHER_TYPE_IPV4 as u16) << 8);
        if ether_type == ether_type_ipv4 {
            // IP 头部在以太网头部之后
            let ip_hdr_ptr = eth_hdr_ptr.add(RTE_ETHER_HDR_LEN as usize);
            
            // IPv4 头部结构:
            // struct rte_ipv4_hdr {
            //     uint8_t  version_ihl;   /**< version and header length */
            //     uint8_t  type_of_service;/**< type of service */
            //     uint16_t total_length;  /**< length of packet */
            //     uint16_t packet_id;     /**< packet ID */
            //     uint16_t fragment_offset;/**< fragmentation offset */
            //     uint8_t  time_to_live;  /**< time to live */
            //     uint8_t  next_proto_id; /**< protocol ID */
            //     uint16_t hdr_checksum;  /**< header checksum */
            //     uint32_t src_addr;      /**< source address */
            //     uint32_t dst_addr;      /**< destination address */
            // }
            
            // 源 IP 地址在 IPv4 头部偏移 12 字节的位置
            // 使用 read_unaligned 和 write_unaligned 处理可能未对齐的内存访问
            let src_ip_ptr = ip_hdr_ptr.add(12) as *mut u32;
            let dst_ip_ptr = ip_hdr_ptr.add(16) as *mut u32;
            
            // 交换源和目标 IP 地址
            let tmp_ip = ptr::read_unaligned(src_ip_ptr);
            ptr::write_unaligned(src_ip_ptr, ptr::read_unaligned(dst_ip_ptr));
            ptr::write_unaligned(dst_ip_ptr, tmp_ip);
            
            // 重新计算 IP 校验和
            // 校验和在 IPv4 头部偏移 10 字节的位置
            let checksum_ptr = ip_hdr_ptr.add(10) as *mut u16;
            ptr::write_unaligned(checksum_ptr, 0);
            
            // 调用 DPDK 函数计算校验和
            let new_checksum = unsafe { rte_ipv4_cksum(ip_hdr_ptr as *const rte_ipv4_hdr) };
            ptr::write_unaligned(checksum_ptr, new_checksum);
            
            // 获取协议类型 (TCP/UDP)
            // 协议类型在 IPv4 头部偏移 9 字节的位置
            let proto_ptr = ip_hdr_ptr.add(9) as *const u8;
            let proto = *proto_ptr;
            
            // 获取 IPv4 头部长度
            // 头部长度在 version_ihl 字段的低 4 位
            let ihl_ptr = ip_hdr_ptr as *const u8;
            let ihl = (*ihl_ptr & 0x0F) as usize * 4;
            
            // 如果是 TCP 数据包，交换源和目标端口
            if proto == IPPROTO_TCP as u8 {
                // TCP 头部在 IP 头部之后
                let tcp_hdr_ptr = ip_hdr_ptr.add(ihl);
                
                // TCP 头部结构:
                // struct rte_tcp_hdr {
                //     uint16_t src_port;  /**< TCP source port. */
                //     uint16_t dst_port;  /**< TCP destination port. */
                //     uint32_t sent_seq;  /**< TX data sequence number. */
                //     uint32_t recv_ack;  /**< RX data acknowledgement sequence number. */
                //     uint8_t  data_off;  /**< Data offset. */
                //     uint8_t  tcp_flags; /**< TCP flags */
                //     uint16_t rx_win;    /**< RX flow control window. */
                //     uint16_t cksum;     /**< TCP checksum. */
                //     uint16_t tcp_urp;   /**< TCP urgent pointer, if any. */
                // }
                
                // 交换源和目标端口
                // 源端口在 TCP 头部偏移 0 字节的位置
                // 目标端口在 TCP 头部偏移 2 字节的位置
                let src_port_ptr = tcp_hdr_ptr as *mut u16;
                let dst_port_ptr = tcp_hdr_ptr.add(2) as *mut u16;
                
                let tmp_port = ptr::read_unaligned(src_port_ptr);
                ptr::write_unaligned(src_port_ptr, ptr::read_unaligned(dst_port_ptr));
                ptr::write_unaligned(dst_port_ptr, tmp_port);
                
                // 重新计算 TCP 校验和
                // 校验和在 TCP 头部偏移 16 字节的位置
                let tcp_checksum_ptr = tcp_hdr_ptr.add(16) as *mut u16;
                ptr::write_unaligned(tcp_checksum_ptr, 0);
                
                // 调用 DPDK 函数计算校验和
                let tcp_cksum = unsafe { rte_ipv4_udptcp_cksum(
                    ip_hdr_ptr as *const rte_ipv4_hdr,
                    tcp_hdr_ptr as *const c_void
                ) };
                ptr::write_unaligned(tcp_checksum_ptr, tcp_cksum);
            }
            // 如果是 UDP 数据包，交换源和目标端口
            else if proto == IPPROTO_UDP as u8 {
                // UDP 头部在 IP 头部之后
                let udp_hdr_ptr = ip_hdr_ptr.add(ihl);
                
                // UDP 头部结构:
                // struct rte_udp_hdr {
                //     uint16_t src_port;    /**< UDP source port. */
                //     uint16_t dst_port;    /**< UDP destination port. */
                //     uint16_t dgram_len;   /**< UDP datagram length */
                //     uint16_t dgram_cksum; /**< UDP datagram checksum */
                // }
                
                // 交换源和目标端口
                // 源端口在 UDP 头部偏移 0 字节的位置
                // 目标端口在 UDP 头部偏移 2 字节的位置
                let src_port_ptr = udp_hdr_ptr as *mut u16;
                let dst_port_ptr = udp_hdr_ptr.add(2) as *mut u16;
                
                let tmp_port = ptr::read_unaligned(src_port_ptr);
                ptr::write_unaligned(src_port_ptr, ptr::read_unaligned(dst_port_ptr));
                ptr::write_unaligned(dst_port_ptr, tmp_port);
                
                // 重新计算 UDP 校验和
                // 校验和在 UDP 头部偏移 6 字节的位置
                let udp_checksum_ptr = udp_hdr_ptr.add(6) as *mut u16;
                ptr::write_unaligned(udp_checksum_ptr, 0);
                
                // 调用 DPDK 函数计算校验和
                let udp_cksum = unsafe { rte_ipv4_udptcp_cksum(
                    ip_hdr_ptr as *const rte_ipv4_hdr,
                    udp_hdr_ptr as *const c_void
                ) };
                ptr::write_unaligned(udp_checksum_ptr, udp_cksum);
            }
        }
    }
}

// 检查并打印数据包负载
fn check_packet_payload(mbuf: *mut rte_mbuf) {
    unsafe {
        // 获取数据包指针
        let data_ptr = ((*mbuf).buf_addr as *mut u8).add((*mbuf).data_off as usize);
        
        // 检查是否是 IPv4 数据包
        let eth_hdr_ptr = data_ptr;
        let eth_type = ((*eth_hdr_ptr.add(12) as u16) << 8) | (*eth_hdr_ptr.add(13) as u16);
        
        if eth_type != 0x0800 {
            // 不是 IPv4 数据包
            return;
        }
        
        // 获取 IP 头部
        let ip_hdr_ptr = eth_hdr_ptr.add(RTE_ETHER_HDR_LEN as usize);
        let ip_proto = *ip_hdr_ptr.add(9);
        
        if ip_proto != 17 {
            // 不是 UDP 数据包
            return;
        }
        
        // 获取 IP 头部长度
        let ihl = (*ip_hdr_ptr & 0x0F) * 4;
        
        // 获取 UDP 头部
        let udp_hdr_ptr = ip_hdr_ptr.add(ihl as usize);
        
        // 获取 UDP 负载
        let payload_ptr = udp_hdr_ptr.add(8);
        
        // 尝试提取 PKT-XXX 格式的负载
        let mut payload_str = String::new();
        let mut i = 0;
        
        // 最多读取 32 字节
        while i < 32 {
            let byte = *payload_ptr.add(i);
            if byte == 0 {
                break;
            }
            payload_str.push(byte as char);
            i += 1;
        }
        
        // 检查是否包含 PKT- 前缀
        if payload_str.starts_with("PKT-") {
            if let Some(id_str) = payload_str.strip_prefix("PKT-") {
                if let Ok(id) = id_str.parse::<u32>() {
                    // 提取剩余的负载内容（最多显示20个字符）
                    let remaining = if payload_str.len() > 10 {
                        &payload_str[4..std::cmp::min(payload_str.len(), 24)]
                    } else {
                        &id_str
                    };
                    
                    println!("收到数据包 ID: {}, 完整负载: {}", id, payload_str);
                    
                    // 更新数据包追踪器
                    if let Some(tracker) = unsafe { &PACKET_TRACKER } {
                        tracker.lock().unwrap().insert(id, payload_str.clone());
                    }
                }
            }
        }
    }
}

// 生成随机数据包
fn generate_random_packet(mbuf: *mut rte_mbuf, packet_id: u32, rng: &mut ThreadRng) -> u16 {
    unsafe {
        // 获取数据包缓冲区指针
        let data_ptr = ((*mbuf).buf_addr as *mut u8).add((*mbuf).data_off as usize);
        
        // 生成以太网头部
        let eth_hdr_ptr = data_ptr;
        // 目标 MAC 地址 (随机生成)
        for i in 0..6 {
            *eth_hdr_ptr.add(i) = rng.gen::<u8>();
        }
        // 源 MAC 地址 (随机生成)
        for i in 0..6 {
            *eth_hdr_ptr.add(i + 6) = rng.gen::<u8>();
        }
        // 以太网类型 (IPv4 = 0x0800)
        let eth_type_bytes = (0x0800u16).to_be_bytes();
        for i in 0..2 {
            *eth_hdr_ptr.add(12 + i) = eth_type_bytes[i];
        }
        
        // 生成 IP 头部
        let ip_hdr_ptr = eth_hdr_ptr.add(RTE_ETHER_HDR_LEN as usize);
        // 版本和头部长度 (IPv4 = 0x45)
        *ip_hdr_ptr = 0x45;
        // 服务类型
        *(ip_hdr_ptr.add(1)) = 0;
        
        // 总长度 (IP 头部 + UDP 头部 + 数据长度)
        let ip_total_len: u16 = 20 + 8 + 32; // IP 头部 + UDP 头部 + 数据长度
        let ip_total_len_bytes = ip_total_len.to_be_bytes();
        for i in 0..2 {
            *ip_hdr_ptr.add(2 + i) = ip_total_len_bytes[i];
        }
        
        // 标识
        let ip_id = rng.gen::<u16>().to_be_bytes();
        for i in 0..2 {
            *ip_hdr_ptr.add(4 + i) = ip_id[i];
        }
        
        // 标志和片偏移
        let flags_offset = 0u16.to_be_bytes();
        for i in 0..2 {
            *ip_hdr_ptr.add(6 + i) = flags_offset[i];
        }
        
        // 生存时间
        *(ip_hdr_ptr.add(8)) = 64;
        // 协议 (UDP = 17)
        *(ip_hdr_ptr.add(9)) = 17;
        
        // 校验和 (先设置为 0，后面计算)
        let zero_cksum = 0u16.to_be_bytes();
        for i in 0..2 {
            *ip_hdr_ptr.add(10 + i) = zero_cksum[i];
        }
        
        // 源 IP 地址 (192.168.1.1)
        let src_ip = 0xC0A80101u32.to_be_bytes();
        for i in 0..4 {
            *ip_hdr_ptr.add(12 + i) = src_ip[i];
        }
        
        // 目标 IP 地址 (192.168.1.2)
        let dst_ip = 0xC0A80102u32.to_be_bytes();
        for i in 0..4 {
            *ip_hdr_ptr.add(16 + i) = dst_ip[i];
        }
        
        // 计算 IP 校验和
        let ip_cksum = rte_ipv4_cksum(ip_hdr_ptr as *const rte_ipv4_hdr);
        let ip_cksum_bytes = ip_cksum.to_be_bytes();
        for i in 0..2 {
            *ip_hdr_ptr.add(10 + i) = ip_cksum_bytes[i];
        }
        
        // 生成 UDP 头部
        let udp_hdr_ptr = ip_hdr_ptr.add(20); // IP 头部后面
        
        // 源端口 (随机)
        let src_port: u16 = rng.gen_range(1024..65535);
        let src_port_bytes = src_port.to_be_bytes();
        for i in 0..2 {
            *udp_hdr_ptr.add(i) = src_port_bytes[i];
        }
        
        // 目标端口 (随机)
        let dst_port: u16 = rng.gen_range(1024..65535);
        let dst_port_bytes = dst_port.to_be_bytes();
        for i in 0..2 {
            *udp_hdr_ptr.add(2 + i) = dst_port_bytes[i];
        }
        
        // 长度 (UDP 头部 + 数据长度)
        let udp_len: u16 = 8 + 32; // UDP 头部 + 数据长度
        let udp_len_bytes = udp_len.to_be_bytes();
        for i in 0..2 {
            *udp_hdr_ptr.add(4 + i) = udp_len_bytes[i];
        }
        
        // 校验和 (先设置为 0，后面计算)
        let zero_udp_cksum = 0u16.to_be_bytes();
        for i in 0..2 {
            *udp_hdr_ptr.add(6 + i) = zero_udp_cksum[i];
        }
        
        // 生成数据负载
        let payload_ptr = udp_hdr_ptr.add(8); // UDP 头部后面
        
        // 创建格式化的字符串 "PKT-{packet_id}"
        let prefix = format!("PKT-{}", packet_id);
        let prefix_bytes = prefix.as_bytes();
        let prefix_len = prefix_bytes.len();
        
        // 复制前缀到负载
        for i in 0..prefix_len {
            *payload_ptr.add(i) = prefix_bytes[i];
        }
        
        // 填充剩余空间为随机可打印字符
        let printable_chars = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
        for i in prefix_len..32 {
            let random_index = rng.gen_range(0..printable_chars.len());
            *payload_ptr.add(i) = printable_chars[random_index];
        }
        
        // 计算 UDP 校验和
        let udp_cksum = rte_ipv4_udptcp_cksum(
            ip_hdr_ptr as *const rte_ipv4_hdr,
            udp_hdr_ptr as *const c_void
        );
        let udp_cksum_bytes = udp_cksum.to_be_bytes();
        for i in 0..2 {
            *udp_hdr_ptr.add(6 + i) = udp_cksum_bytes[i];
        }
        
        // 设置 mbuf 的长度
        let total_len = RTE_ETHER_HDR_LEN as u16 + ip_total_len;
        (*mbuf).pkt_len = total_len as u32;
        (*mbuf).data_len = total_len;
        
        total_len
    }
}

fn main() {
    println!("启动 DPDK 数据包转发器...");

    // 初始化 DPDK EAL
    let args = vec![
        CString::new("packet_forwarder").unwrap(),
        CString::new("-l").unwrap(),
        CString::new("0-3").unwrap(),  // 使用核心 0-3
        // 使用 PA 模式
        CString::new("--huge-dir=/dev/hugepages").unwrap(),  // 指定大页内存目录
        CString::new("--socket-mem=128").unwrap(),  // 为 socket 0 分配 128MB 内存
        // 使用已经绑定到 DPDK 的物理网卡
    ];

    let mut c_args: Vec<*mut c_char> = args
        .iter()
        .map(|arg| arg.as_ptr() as *mut c_char)
        .collect();

    println!("初始化 DPDK EAL...");
    let ret = unsafe { rte_eal_init(c_args.len() as c_int, c_args.as_mut_ptr()) };
    if ret < 0 {
        eprintln!("无法初始化 EAL: {}", ret);
        return;
    }

    // 设置信号处理器
    unsafe {
        // 使用 std::io::stderr 代替 libc::stderr
        let stderr_ptr = std::ptr::null_mut();
        rte_openlog_stream(stderr_ptr);
        libc::signal(libc::SIGINT, signal_handler as usize);
        libc::signal(libc::SIGTERM, signal_handler as usize);
    }

    // 检查可用端口
    let nb_ports = unsafe { rte_eth_dev_count_avail() };
    println!("发现 {} 个网络端口", nb_ports);

    if nb_ports < 2 {
        eprintln!("需要至少两个网络端口进行转发");
        unsafe { rte_eal_cleanup() };
        return;
    }

    // 为接收队列分配内存池
    let pool_name = CString::new("mbuf_pool").unwrap();
    let mp = unsafe {
        rte_pktmbuf_pool_create(
            pool_name.as_ptr(),
            8192 * nb_ports as u32,  // 元素数量
            256,                     // 缓存大小
            0,                       // 私有数据大小
            RTE_MBUF_DEFAULT_BUF_SIZE as u16,
            rte_socket_id() as i32,
        )
    };
    if mp.is_null() {
        eprintln!("无法创建 mbuf 池");
        unsafe { rte_eal_cleanup() };
        return;
    }

    // 配置所有端口
    for port_id in 0..nb_ports {
        println!("初始化端口 {}...", port_id);
        
        // 配置以太网设备
        let mut port_conf: rte_eth_conf = unsafe { std::mem::zeroed() };
        port_conf.rxmode.max_rx_pkt_len = RTE_ETHER_MAX_LEN as u32;

        let ret = unsafe {
            rte_eth_dev_configure(
                port_id,
                1, // 接收队列数量
                1, // 发送队列数量
                &port_conf,
            )
        };
        if ret < 0 {
            eprintln!("无法配置端口 {}: {}", port_id, ret);
            unsafe { rte_eal_cleanup() };
            return;
        }

        // 调整接收和发送描述符数量
        let mut rx_desc = 128; // 接收描述符数量
        let mut tx_desc = 512; // 发送描述符数量
        unsafe {
            rte_eth_dev_adjust_nb_rx_tx_desc(port_id, &mut rx_desc, &mut tx_desc);
        }

        // 设置接收队列
        let rx_queue_id = 0;
        let ret = unsafe {
            rte_eth_rx_queue_setup(
                port_id,
                rx_queue_id,
                rx_desc,
                rte_eth_dev_socket_id(port_id) as u32,
                ptr::null(),
                mp,
            )
        };
        if ret < 0 {
            eprintln!("无法设置端口 {} 的接收队列: {}", port_id, ret);
            unsafe { rte_eal_cleanup() };
            return;
        }

        // 设置发送队列
        let tx_queue_id = 0;
        let ret = unsafe {
            rte_eth_tx_queue_setup(
                port_id,
                tx_queue_id,
                tx_desc,
                rte_eth_dev_socket_id(port_id) as u32,
                ptr::null(),
            )
        };
        if ret < 0 {
            eprintln!("无法设置端口 {} 的发送队列: {}", port_id, ret);
            unsafe { rte_eal_cleanup() };
            return;
        }

        // 启动端口
        let ret = unsafe { rte_eth_dev_start(port_id) };
        if ret < 0 {
            eprintln!("无法启动端口 {}: {}", port_id, ret);
            unsafe { rte_eal_cleanup() };
            return;
        }

        // 获取端口 MAC 地址
        let mut mac_addr: rte_ether_addr = unsafe { std::mem::zeroed() };
        unsafe { rte_eth_macaddr_get(port_id, &mut mac_addr) };
        
        println!(
            "端口 {} MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            port_id,
            mac_addr.addr_bytes[0],
            mac_addr.addr_bytes[1],
            mac_addr.addr_bytes[2],
            mac_addr.addr_bytes[3],
            mac_addr.addr_bytes[4],
            mac_addr.addr_bytes[5]
        );

        // 启用混杂模式
        unsafe { rte_eth_promiscuous_enable(port_id) };
    }

    println!("所有端口初始化完成");
    println!("开始数据包转发...");
    println!("按 Ctrl+C 退出");

    // 主循环：接收和转发数据包
    let force_quit = Arc::new(AtomicBool::new(false));
    let force_quit_clone = force_quit.clone();

    // 创建一个线程来检查 FORCE_QUIT 变量
    thread::spawn(move || {
        while !unsafe { FORCE_QUIT.load(Ordering::SeqCst) } {
            thread::sleep(Duration::from_millis(100));
        }
        force_quit_clone.store(true, Ordering::SeqCst);
    });

    // 统计信息
    let mut total_rx_packets = vec![0; nb_ports as usize];
    let mut total_tx_packets = vec![0; nb_ports as usize];

    // 初始化数据包跟踪器
    unsafe {
        PACKET_TRACKER = Some(Arc::new(Mutex::new(HashMap::new())));
    }
    
    // 创建数据包生成线程
    let tx_port = 0;
    let tx_queue = 0;
    let mbuf_pool_ptr = mp as usize;
    let force_quit_gen = force_quit.clone();
    
    let packet_gen_thread = thread::spawn(move || {
        let mut packet_id: u32 = 0;
        let mut rng = rand::thread_rng();
        let mbuf_pool = mbuf_pool_ptr as *mut rte_mempool;
        
        // 每秒生成 10 个数据包
        while !force_quit_gen.load(Ordering::SeqCst) {
            // 分配一个 mbuf
            let mut mbuf = unsafe { rte_pktmbuf_alloc(mbuf_pool) };
            if mbuf.is_null() {
                println!("无法分配 mbuf");
                thread::sleep(Duration::from_millis(100));
                continue;
            }
            
            // 生成随机数据包
            let _pkt_len = generate_random_packet(mbuf, packet_id, &mut rng);
            
            // 发送数据包
            let nb_tx = unsafe { rte_eth_tx_burst(tx_port, tx_queue, &mut mbuf, 1) };
            
            if nb_tx == 0 {
                // 如果发送失败，释放 mbuf
                unsafe { rte_pktmbuf_free(mbuf) };
                println!("发送数据包失败");
            } else {
                println!("发送数据包: PKT-{}", packet_id);
                
                // 更新数据包 ID
                packet_id += 1;
            }
            
            // 等待一段时间再发送下一个数据包
            thread::sleep(Duration::from_millis(100));
        }
        println!("数据包生成线程退出");
    });

    // 开始数据包转发
    println!("开始数据包转发...");
    println!("按 Ctrl+C 退出");
    
    // 添加计时器，每秒打印一次统计信息
    let mut last_print_time = Instant::now();
    let print_interval = Duration::from_secs(1);
    
    // 添加详细日志的计数器
    let mut detailed_log_counter = 0;
    let detailed_log_interval = 1000; // 每处理1000个包打印一次详细信息

    // 数据包转发主循环
    while !force_quit.load(Ordering::SeqCst) {
        // 处理所有端口
        for port_id in 0..nb_ports {
            // 接收数据包
            let mut rx_mbufs: [*mut rte_mbuf; 32] = [ptr::null_mut(); 32];
            let nb_rx = unsafe {
                rte_eth_rx_burst(
                    port_id,
                    0, // 接收队列 ID
                    rx_mbufs.as_mut_ptr(),
                    rx_mbufs.len() as u16,
                )
            };

            if nb_rx > 0 {
                detailed_log_counter += nb_rx as usize;
                total_rx_packets[port_id as usize] += nb_rx as usize;
                
                // 处理每个接收到的数据包
                for i in 0..nb_rx {
                    let pkt = rx_mbufs[i as usize];
                    
                    // 检查并打印数据包负载
                    check_packet_payload(pkt);
                    
                    // 处理数据包
                    process_packet(pkt);
                }
                
                // 发送处理后的数据包
                let dst_port = (port_id + 1) % nb_ports;
                let nb_tx = unsafe {
                    rte_eth_tx_burst(
                        dst_port,
                        0,
                        rx_mbufs.as_ptr() as *mut *mut rte_mbuf,
                        nb_rx as u16,
                    )
                };
                
                total_tx_packets[dst_port as usize] += nb_tx as usize;
                
                // 释放未发送的数据包
                if nb_tx < nb_rx {
                    for i in nb_tx..nb_rx {
                        unsafe { rte_pktmbuf_free(rx_mbufs[i as usize]) };
                    }
                }
                
                // 如果需要打印详细日志
                if detailed_log_counter >= detailed_log_interval {
                    println!("处理了 {} 个数据包 (端口{}->端口{})", detailed_log_counter, port_id, dst_port);
                    detailed_log_counter = 0;
                }
            }
        }
        
        // 每秒打印一次统计信息
        let now = Instant::now();
        if now.duration_since(last_print_time) >= print_interval {
            for port_id in 0..nb_ports {
                println!("实时统计 - 端口 {}: 接收 {} 个数据包，发送 {} 个数据包",
                    port_id, total_rx_packets[port_id as usize], total_tx_packets[port_id as usize]);
            }
            last_print_time = now;
        }
        
        // 短暂休眠，避免 CPU 使用率过高
        thread::sleep(Duration::from_millis(1));
    }

    println!("清理资源...");
    
    // 等待数据包生成线程结束
    // 注意：由于 packet_gen_thread 在 loop 中运行，我们不能 join，但它会在检测到 force_quit 为 true 时退出
    
    // 打印统计信息
    for port_id in 0..nb_ports {
        println!(
            "端口 {}: 接收 {} 个数据包，发送 {} 个数据包",
            port_id,
            total_rx_packets[port_id as usize],
            total_tx_packets[port_id as usize]
        );
        
        // 停止端口
        unsafe {
            rte_eth_dev_stop(port_id);
            rte_eth_dev_close(port_id);
        }
    }

    // 释放内存池
    unsafe {
        if !mp.is_null() {
            rte_mempool_free(mp);
        }
    }

    // 清理 EAL
    unsafe { rte_eal_cleanup() };
    println!("程序退出");
}
