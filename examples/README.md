# DPDK Rust 示例程序

这个目录包含了几个使用 DPDK Rust 绑定的示例程序，展示了如何使用 DPDK 的各种功能。

## 前提条件

在运行这些示例之前，确保你已经：

1. 安装了 DPDK 及其依赖项
2. 成功编译了 DPDK Rust 绑定库
3. 如果要使用物理网卡，需要配置大页内存和绑定网卡到 DPDK 兼容的驱动程序

## 示例程序

### 1. 基本 DPDK 示例 (basic_dpdk.rs)

这个示例展示了如何初始化 DPDK 环境、配置网络端口并接收/发送数据包。

运行方法：
```bash
sudo cargo run --example basic_dpdk
```

### 2. 数据包转发器 (packet_forwarder.rs)

这个示例实现了一个简单的数据包转发应用程序，它可以在两个网络端口之间转发数据包，并修改数据包的 MAC、IP 和端口信息。

#### packet_forwarder

这个示例展示了如何使用 DPDK 实现一个简单的数据包转发器，它会接收数据包并将其转发到另一个端口。

##### 功能特点

1. **基本数据包转发**：接收数据包并将其转发到另一个端口
2. **MAC 地址交换**：交换源和目标 MAC 地址
3. **IP 地址交换**：交换源和目标 IP 地址
4. **TCP/UDP 端口交换**：交换源和目标端口
5. **随机数据包生成**：生成随机数据包并发送，用于测试
6. **数据包负载检查**：检查并打印接收到的数据包负载信息

##### 随机数据包生成

程序包含一个数据包生成线程，它会定期生成随机数据包并发送。这些数据包具有以下特点：

- 随机生成的以太网和 IP 头部
- 使用 UDP 协议
- 负载格式为 `PKT-{packet_id}`，后面跟随随机字符
- 每 100 毫秒生成一个新数据包

##### 数据包负载检查

当程序接收到数据包时，它会检查数据包负载并打印出包含 `PKT-` 前缀的数据包信息，这有助于验证数据包转发功能是否正常工作。

##### 运行方法

由于 DPDK 需要访问网络设备和大页内存，运行此程序需要 sudo 权限：

```bash
# 编译程序
cargo build --example packet_forwarder

# 运行程序
sudo LD_LIBRARY_PATH=/usr/local/lib64 ./target/debug/examples/packet_forwarder
```

##### 注意事项

- 程序需要至少两个网络端口才能进行转发
- 需要预先配置 DPDK 环境，包括大页内存和网络设备绑定
- 使用 Ctrl+C 可以优雅地退出程序

运行方法：
```bash
sudo cargo run --example packet_forwarder
```

### 3. 内存池演示 (mempool_demo.rs)

这个示例展示了如何使用 DPDK 的内存池功能，包括创建内存池、分配和释放对象。

运行方法：
```bash
cargo run --example mempool_demo
```

## 注意事项

1. 大多数 DPDK 应用程序需要以 root 权限运行
2. 如果使用物理网卡，需要先配置好 DPDK 环境
3. 这些示例默认使用虚拟设备（如 pcap），适合在没有专用硬件的环境中测试
4. 按 Ctrl+C 可以优雅地退出程序

## 自定义配置

如果需要自定义配置（如使用不同的网卡或更改核心分配），可以修改示例代码中的 EAL 初始化参数。

例如，要使用物理网卡，可以将：
```rust
CString::new("--vdev=net_pcap0,iface=lo").unwrap(),
```

替换为：
```rust
CString::new("--allow").unwrap(),
CString::new("0000:01:00.0").unwrap(),
```

其中 `0000:01:00.0` 是你的网卡的 PCI 地址。

## 故障排除

如果遇到权限问题，确保以 root 权限运行或使用 sudo。

如果遇到 "No available ports" 错误，检查网卡是否已绑定到 DPDK 兼容的驱动程序。

如果遇到内存相关错误，确保已配置足够的大页内存。
