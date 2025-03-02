//! Rust 绑定库，用于 DPDK (Data Plane Development Kit)
//!
//! 这个库提供了对 DPDK C 库的 Rust 绑定，允许在 Rust 中使用 DPDK 的功能。
//! DPDK 是一个高性能的数据包处理框架，专为高速网络应用设计。

// 重新导出 dpdk-sys 中的所有内容
pub use dpdk_sys::*;

// 添加一些辅助函数和安全包装器
pub mod utils {
    use super::*;
    use std::ffi::CString;
    use std::os::raw::{c_char, c_int};
    
    /// 将 Rust 字符串转换为 C 字符串数组，用于 EAL 初始化
    pub fn args_to_c_array(args: &[String]) -> (Vec<*mut c_char>, usize) {
        let c_args: Vec<*mut c_char> = args
            .iter()
            .map(|arg| CString::new(arg.as_str()).unwrap().into_raw())
            .collect();
        
        let len = c_args.len();
        (c_args, len)
    }
    
    /// 初始化 DPDK EAL 的安全包装器
    pub fn eal_init(args: &[String]) -> Result<i32, i32> {
        let (mut c_args, argc) = args_to_c_array(args);
        
        let ret = unsafe { rte_eal_init(argc as c_int, c_args.as_mut_ptr()) };
        
        // 释放分配的 CString 内存
        for arg in c_args {
            unsafe { let _ = CString::from_raw(arg); }
        }
        
        if ret < 0 {
            Err(ret)
        } else {
            Ok(ret)
        }
    }
    
    /// 获取 DPDK 版本信息的安全包装器
    pub fn get_version() -> String {
        let version = unsafe { std::ffi::CStr::from_ptr(rte_version()) };
        version.to_string_lossy().into_owned()
    }
    
    /// 获取 lcore ID 列表的安全包装器
    pub fn get_lcores() -> Vec<u32> {
        let mut lcores = Vec::new();
        let mut lcore_id: u32 = 0;
        
        unsafe {
            rte_eal_mp_remote_launch(None, std::ptr::null_mut(), 0);
            
            while rte_eal_get_lcore_state(lcore_id) != 0 {
                lcores.push(lcore_id);
                lcore_id += 1;
                
                if lcore_id >= RTE_MAX_LCORE {
                    break;
                }
            }
        }
        
        lcores
    }
}
