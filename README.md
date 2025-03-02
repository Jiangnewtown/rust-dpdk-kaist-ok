# rust-dpdk

[![Build Status](https://github.com/ANLAB-KAIST/rust-dpdk/actions/workflows/build.yaml/badge.svg)](https://github.com/ANLAB-KAIST/rust-dpdk/actions/workflows/build.yaml)

已在 <https://github.com/DPDK/dpdk.git> v22.11 版本上测试通过。

## 设计目标

市面上有其他的 `rust-dpdk` 实现，您可以根据自己的需求选择最合适的实现。
(https://github.com/flier/rust-dpdk, https://github.com/netsys/netbricks)
本库基于以下设计目标构建：

1. 最小化手写绑定代码。
2. 不在此仓库中包含 `bindgen` 的输出。
3. 静态链接 DPDK 库而不是使用共享库。

| Library   | No bindgen output | Static linking  | Inline function wrappers | Prevent PMD opt-out |
| --------- | ----------------- | --------------- | ------------------------ | ------------------- |
| flier     | bindgen snapshot  | O               | O (manual)               | X                   |
| netbricks | manual FFI        | X               | X                        | O (via dynload)     |
| ANLAB     | ondemand creation | O               | O (automatic)            | O                   |

## 前提条件

首先，本库依赖于 Intel Data Plane Development Kit (DPDK)。
请参考官方 DPDK 文档安装 DPDK (http://doc.dpdk.org/guides/linux_gsg/index.html)。

以下是构建 DPDK 和使用本库的基本说明。

### 安装依赖

#### Fedora 安装依赖（推荐）

```bash
# 安装基本构建工具和依赖
sudo dnf install -y git make gcc kernel-devel kernel-headers numactl-devel meson python3-pyelftools

# 安装 DPDK 开发所需的库
sudo dnf install -y clang clang-devel llvm-devel pkgconfig

# 安装 libbsd 依赖
sudo dnf install -y libbsd-devel

# 安装网卡驱动依赖项（可选，根据需要安装）
sudo dnf install -y libpcap-devel          # pcap 依赖
sudo dnf install -y libibverbs-devel rdma-core-devel  # Mellanox 网卡依赖
sudo dnf install -y zlib-devel             # zlib 依赖
sudo dnf install -y libbpf-devel xdp-tools # XDP 和 BPF 依赖
```

#### Ubuntu/Debian 安装依赖

```sh
# 安装基本构建工具和依赖
apt-get install -y curl git build-essential libnuma-dev meson python3-pyelftools 

# 安装内核头文件（用于构建内核驱动）
apt-get install -y linux-headers-`uname -r` 

# 安装 DPDK 开发所需的库
apt-get install -y libclang-dev clang llvm-dev pkg-config 

# 安装 libbsd 依赖
apt-get install -y libbsd-dev 

# 安装网卡驱动依赖项（可选，根据需要安装）
apt-get install -y libpcap-dev          # pcap 依赖
apt-get install -y libibverbs-dev       # Mellanox 网卡依赖
apt-get install -y zlib1g-dev           # zlib 依赖
```

### 安装 DPDK

可以通过以下命令安装 DPDK：
```{.sh}
meson build
ninja -C build
ninja -C build install # 需要 sudo 权限
```

从 v20.11 开始，内核驱动被移至 https://git.dpdk.org/dpdk-kmods/。
如果您的网卡需要内核驱动，可以在上述链接中找到。

现在将 `rust-dpdk` 添加到您项目的 `Cargo.toml` 中即可使用！
```toml
[dependencies]
rust-dpdk = { git = "https://github.com/ANLAB-KAIST/rust-dpdk", branch = "main" }
```

## 配置大页内存

DPDK 应用程序需要大页内存支持，可以通过以下命令配置：

```bash
# 配置 2MB 大页内存
sudo sh -c 'echo 1024 > /sys/kernel/mm/hugepages/hugepages-2048kB/nr_hugepages'

# 挂载大页内存文件系统
sudo mkdir -p /dev/hugepages
sudo mount -t hugetlbfs nodev /dev/hugepages
```

## 示例程序

本仓库包含以下示例程序：

1. **Basic DPDK Example**: 演示基本的 DPDK 初始化和清理流程
2. **Packet Forwarder**: 实现一个简单的数据包转发器，可以在两个网络端口之间转发数据包，并修改数据包的 MAC、IP 和端口信息
   - 支持随机数据包生成功能，用于测试和验证数据包转发逻辑
   - 可以检查并打印接收到的数据包负载信息
3. **Memory Pool Demo**: 演示如何创建和使用 DPDK 内存池

详细的示例说明请参考 [examples/README.md](examples/README.md)。

## 运行 DPDK 应用程序

大多数 DPDK 应用程序需要：

1. **Root 权限**：运行示例时使用 `sudo`
2. **大页内存**：为了获得更好的性能，配置大页内存
   ```
   echo 1024 > /sys/kernel/mm/hugepages/hugepages-2048kB/nr_hugepages
   ```
3. **网络设备绑定**：对于物理网卡，将其绑定到 DPDK 兼容的驱动程序
   ```
   sudo dpdk-devbind.py --bind=vfio-pci 0000:01:00.0
   ```

本仓库中的示例默认配置为使用虚拟设备（pcap），因此可以在没有专用硬件的情况下进行测试。

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
