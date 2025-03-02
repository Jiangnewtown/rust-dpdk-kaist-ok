# rust-dpdk

[![Build Status](https://github.com/ANLAB-KAIST/rust-dpdk/actions/workflows/build.yaml/badge.svg)](https://github.com/ANLAB-KAIST/rust-dpdk/actions/workflows/build.yaml)

Tested with <https://github.com/DPDK/dpdk.git> v22.11.

## Goals

There are other `rust-dpdk` implementations and you may choose most proper implementation to your purpose.
(https://github.com/flier/rust-dpdk, https://github.com/netsys/netbricks)
This library is built for following design goals.

1. Minimize hand-written binding code.
1. Do not include `bindgen`'s output in this repository.
1. Statically link DPDK libraries instead of using shared libraries.

| Library   | No bindgen output | Static linking  | Inline function wrappers | Prevent PMD opt-out |
| --------- | ----------------- | --------------- | ------------------------ | ------------------- |
| flier     | bindgen snapshot  | O               | O (manual)               | X                   |
| netbricks | manual FFI        | X               | X                        | O (via dynload)     |
| ANLAB     | ondemand creation | O               | O (automatic)            | O                   |

## Prerequisites

First, this library depends on Intel Data Plane Development Kit (DPDK).
Refer to official DPDK document to install DPDK (http://doc.dpdk.org/guides/linux_gsg/index.html).

Here, we include basic instructions to build DPDK and use this library.

Commonly, following packages are required to build DPDK.
```sh
apt-get install -y curl git build-essential libnuma-dev meson python3-pyelftools # To download and build DPDK
apt-get install -y linux-headers-`uname -r` # To build kernel drivers
apt-get install -y libclang-dev clang llvm-dev pkg-config # To analyze DPDK headers and create bindings
apt-get install -y libbsd-dev # Required for some DPDK functions
```

DPDK can be installed by following commands:
```{.sh}
meson build
ninja -C build
ninja -C build install # sudo required
```
Since v20.11, kernel drivers are moved to https://git.dpdk.org/dpdk-kmods/.
If your NIC requires kernel drivers, they are found at the above link.


Now add `rust-dpdk` to your project's `Cargo.toml` and use it!
```toml
[dependencies]
rust-dpdk = { git = "https://github.com/ANLAB-KAIST/rust-dpdk", branch = "main" }
```

## 示例程序

本仓库包含以下示例程序：

1. **Basic DPDK Example**: 演示基本的 DPDK 初始化和清理流程
2. **Packet Forwarder**: 实现一个简单的数据包转发器，可以在两个网络端口之间转发数据包，并修改数据包的 MAC、IP 和端口信息
   - 支持随机数据包生成功能，用于测试和验证数据包转发逻辑
   - 可以检查并打印接收到的数据包负载信息
3. **Memory Pool Demo**: 演示如何创建和使用 DPDK 内存池

详细的示例说明请参考 [examples/README.md](examples/README.md)。

## Running DPDK Applications

Most DPDK applications require:

1. **Root privileges**: Use `sudo` when running the examples
2. **Huge pages**: Configure huge pages for better performance
   ```
   echo 1024 > /sys/kernel/mm/hugepages/hugepages-2048kB/nr_hugepages
   ```
3. **Network device binding**: For physical NICs, bind them to DPDK-compatible drivers
   ```
   sudo dpdk-devbind.py --bind=vfio-pci 0000:01:00.0
   ```

The examples in this repository are configured to use virtual devices (pcap) by default, so they can be tested without dedicated hardware.

## DPDK 网络接口绑定工具使用指南

DPDK 应用程序需要将网络接口绑定到 DPDK 兼容的驱动程序上才能使用。DPDK 提供了 `dpdk-devbind.py` 脚本来管理网络接口的绑定状态。以下是该工具的常用命令：

### 查看网络接口状态

```bash
# 查看所有网络接口的绑定状态
sudo /usr/local/bin/dpdk-devbind.py --status

# 只查看网络接口
sudo /usr/local/bin/dpdk-devbind.py --status-dev net
```

输出示例：
```
Network devices using DPDK-compatible driver
============================================
0000:02:02.0 '82545EM Gigabit Ethernet Controller (Copper) 100f' drv=vfio-pci unused=e1000
0000:02:03.0 '82545EM Gigabit Ethernet Controller (Copper) 100f' drv=vfio-pci unused=e1000

Network devices using kernel driver
===================================
0000:02:01.0 '82545EM Gigabit Ethernet Controller (Copper) 100f' if=ens33 drv=e1000 unused=vfio-pci *Active*
```

### 绑定网络接口到 DPDK 驱动

```bash
# 将指定的网络接口绑定到 vfio-pci 驱动
sudo /usr/local/bin/dpdk-devbind.py --bind=vfio-pci <PCI_ID>

# 示例：绑定两个网络接口
sudo /usr/local/bin/dpdk-devbind.py --bind=vfio-pci 0000:02:02.0 0000:02:03.0
```

### 解绑网络接口（恢复到原始驱动）

```bash
# 将网络接口从 DPDK 驱动解绑，恢复到原始驱动
sudo /usr/local/bin/dpdk-devbind.py --bind=<ORIGINAL_DRIVER> <PCI_ID>

# 示例：将网络接口恢复到 e1000 驱动
sudo /usr/local/bin/dpdk-devbind.py --bind=e1000 0000:02:02.0
```

### 设置 VFIO 权限

如果使用 vfio-pci 驱动，可能需要设置设备权限：

```bash
# 修改 vfio 设备的权限
sudo chmod 666 /dev/vfio/vfio
sudo chmod 666 /dev/vfio/<GROUP_ID>
```

### 常见问题解决

1. **设备或资源忙错误**：如果遇到 "Device or resource busy" 错误，可能是其他 DPDK 进程正在使用该设备，可以使用以下命令查找并终止：
   ```bash
   sudo lsof /dev/vfio/<GROUP_ID>
   sudo kill -9 <PID>
   ```

2. **清理 DPDK 锁文件**：如果 DPDK 应用程序异常退出，可能需要清理锁文件：
   ```bash
   sudo rm -f /var/run/dpdk/rte/config
   sudo rm -f /var/run/dpdk/rte/*.lock
   sudo rm -f /var/run/dpdk/rte/mp_socket
   ```

3. **配置大页内存**：DPDK 应用程序需要大页内存支持：
   ```bash
   # 配置 2MB 大页内存
   sudo sh -c 'echo 1024 > /sys/kernel/mm/hugepages/hugepages-2048kB/nr_hugepages'
   
   # 挂载大页内存文件系统
   sudo mkdir -p /dev/hugepages
   sudo mount -t hugetlbfs nodev /dev/hugepages
   ```

更多详细信息，请参考 [DPDK 官方文档](https://doc.dpdk.org/guides/linux_gsg/linux_drivers.html)。
