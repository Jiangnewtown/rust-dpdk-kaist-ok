use rust_dpdk::*;
use std::ffi::CString;
use std::os::raw::{c_char, c_int};
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

// 全局变量，用于控制程序退出
static mut FORCE_QUIT: AtomicBool = AtomicBool::new(false);

// 安全处理信号
extern "C" fn signal_handler(_signum: c_int) {
    unsafe {
        FORCE_QUIT.store(true, Ordering::SeqCst);
    }
}

fn main() {
    println!("启动 DPDK 示例程序...");

    // 初始化 DPDK EAL
    let args = vec![
        CString::new("basic_dpdk").unwrap(),
        CString::new("-l").unwrap(),
        CString::new("0-1").unwrap(),  // 使用核心 0 和 1
        CString::new("--no-pci").unwrap(),  // 不使用 PCI 设备，适合虚拟环境测试
        CString::new("--vdev=net_pcap0,iface=lo").unwrap(),  // 使用 pcap 驱动，监听本地回环接口
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

    if nb_ports == 0 {
        eprintln!("没有可用的网络端口");
        unsafe { rte_eal_cleanup() };
        return;
    }

    // 配置第一个端口
    let port_id = 0;
    let nb_rxd = 128; // 接收描述符数量
    let nb_txd = 512; // 发送描述符数量

    // 配置以太网设备
    let mut port_conf: rte_eth_conf = unsafe { std::mem::zeroed() };
    port_conf.rxmode.max_rx_pkt_len = RTE_ETHER_MAX_LEN as u32;

    println!("配置端口 {}...", port_id);
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
    let mut rx_desc = nb_rxd;
    let mut tx_desc = nb_txd;
    unsafe {
        rte_eth_dev_adjust_nb_rx_tx_desc(port_id, &mut rx_desc, &mut tx_desc);
    }

    // 为接收队列分配内存池
    let pool_name = CString::new("rx_mbuf_pool").unwrap();
    let mp = unsafe {
        rte_pktmbuf_pool_create(
            pool_name.as_ptr(),
            8192,        // 元素数量
            256,         // 缓存大小
            0,           // 私有数据大小
            RTE_MBUF_DEFAULT_BUF_SIZE as u16,
            rte_socket_id() as i32,
        )
    };
    if mp.is_null() {
        eprintln!("无法创建 mbuf 池");
        unsafe { rte_eal_cleanup() };
        return;
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
        eprintln!("无法设置接收队列: {}", ret);
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
        eprintln!("无法设置发送队列: {}", ret);
        unsafe { rte_eal_cleanup() };
        return;
    }

    // 启动端口
    println!("启动端口 {}...", port_id);
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

    println!("开始接收数据包...");
    println!("按 Ctrl+C 退出");

    // 主循环：接收和处理数据包
    let force_quit = Arc::new(AtomicBool::new(false));
    let force_quit_clone = force_quit.clone();

    // 创建一个线程来检查 FORCE_QUIT 变量
    thread::spawn(move || {
        while !unsafe { FORCE_QUIT.load(Ordering::SeqCst) } {
            thread::sleep(Duration::from_millis(100));
        }
        force_quit_clone.store(true, Ordering::SeqCst);
    });

    // 接收和处理数据包的主循环
    let mut total_rx_packets = 0;
    let mut total_tx_packets = 0;

    while !force_quit.load(Ordering::SeqCst) {
        // 接收数据包
        let mut rx_mbufs: [*mut rte_mbuf; 32] = [ptr::null_mut(); 32];
        let nb_rx = unsafe {
            rte_eth_rx_burst(
                port_id,
                rx_queue_id,
                rx_mbufs.as_mut_ptr(),
                rx_mbufs.len() as u16,
            )
        };

        if nb_rx > 0 {
            total_rx_packets += nb_rx as usize;
            println!("接收到 {} 个数据包，总计: {}", nb_rx, total_rx_packets);

            // 简单处理：将接收到的数据包原样发送回去
            let nb_tx = unsafe {
                rte_eth_tx_burst(
                    port_id,
                    tx_queue_id,
                    rx_mbufs.as_mut_ptr(),
                    nb_rx,
                )
            };

            total_tx_packets += nb_tx as usize;

            // 释放未发送的数据包
            if nb_tx < nb_rx {
                for i in nb_tx..nb_rx {
                    unsafe { rte_pktmbuf_free(rx_mbufs[i as usize]) };
                }
            }
        }

        // 短暂休眠，避免 CPU 使用率过高
        thread::sleep(Duration::from_millis(1));
    }

    println!("清理资源...");
    println!("总接收数据包: {}", total_rx_packets);
    println!("总发送数据包: {}", total_tx_packets);

    // 停止端口
    unsafe {
        rte_eth_dev_stop(port_id);
        rte_eth_dev_close(port_id);
    }

    // 清理 EAL
    unsafe { rte_eal_cleanup() };
    println!("程序退出");
}
