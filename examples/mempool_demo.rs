use rust_dpdk::*;
use std::ffi::CString;
use std::os::raw::{c_char, c_int, c_void};
use std::ptr;

fn main() {
    println!("启动 DPDK 内存池示例...");

    // 初始化 DPDK EAL
    let args = vec![
        CString::new("mempool_demo").unwrap(),
        CString::new("-l").unwrap(),
        CString::new("0").unwrap(),  // 只使用核心 0
        CString::new("--no-pci").unwrap(),  // 不使用 PCI 设备
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

    // 创建内存池
    let pool_name = CString::new("test_mempool").unwrap();
    let n = 1024;  // 池中元素的数量
    let elt_size = 1024;  // 每个元素的大小（字节）
    let cache_size = 32;  // 每个核心的缓存大小
    
    println!("创建内存池: {}", pool_name.to_str().unwrap());
    println!("  元素数量: {}", n);
    println!("  元素大小: {} 字节", elt_size);
    println!("  缓存大小: {}", cache_size);
    
    let mp = unsafe {
        rte_mempool_create(
            pool_name.as_ptr(),
            n,
            elt_size,
            cache_size,
            0,  // 私有数据大小
            None,  // 没有初始化回调
            ptr::null_mut(),  // 没有初始化参数
            None,  // 没有对象构造回调
            ptr::null_mut(),  // 没有对象构造参数
            rte_socket_id() as i32,  // 使用当前 NUMA 节点
            0,  // 标志
        )
    };
    
    if mp.is_null() {
        eprintln!("无法创建内存池");
        unsafe { rte_eal_cleanup() };
        return;
    }
    
    // 获取内存池信息
    let avail = unsafe { rte_mempool_avail_count(mp) };
    let in_use = unsafe { rte_mempool_in_use_count(mp) };
    
    println!("内存池创建成功:");
    println!("  可用元素: {}", avail);
    println!("  使用中元素: {}", in_use);
    
    // 分配一些对象
    let num_obj = 10;
    let mut obj_table: Vec<*mut c_void> = vec![ptr::null_mut(); num_obj];
    
    println!("从内存池分配 {} 个对象...", num_obj);
    let ret = unsafe {
        rte_mempool_get_bulk(mp, obj_table.as_mut_ptr() as *mut *mut c_void, num_obj as u32)
    };
    
    if ret < 0 {
        eprintln!("无法从内存池分配对象: {}", ret);
        unsafe { rte_eal_cleanup() };
        return;
    }
    
    // 再次获取内存池信息
    let avail = unsafe { rte_mempool_avail_count(mp) };
    let in_use = unsafe { rte_mempool_in_use_count(mp) };
    
    println!("分配后内存池状态:");
    println!("  可用元素: {}", avail);
    println!("  使用中元素: {}", in_use);
    
    // 使用分配的对象
    for i in 0..num_obj {
        let obj = obj_table[i];
        println!("对象 {}: 地址 {:p}", i, obj);
        
        // 写入一些数据
        unsafe {
            let data = obj as *mut u8;
            for j in 0..16 {
                *data.add(j) = (i * 10 + j) as u8;
            }
        }
    }
    
    // 释放对象
    println!("释放对象回内存池...");
    unsafe {
        rte_mempool_put_bulk(mp, obj_table.as_mut_ptr() as *mut *mut c_void, num_obj as u32);
    }
    
    // 最后获取内存池信息
    let avail = unsafe { rte_mempool_avail_count(mp) };
    let in_use = unsafe { rte_mempool_in_use_count(mp) };
    
    println!("释放后内存池状态:");
    println!("  可用元素: {}", avail);
    println!("  使用中元素: {}", in_use);
    
    // 清理 EAL
    println!("清理资源...");
    unsafe { rte_eal_cleanup() };
    println!("程序退出");
}
