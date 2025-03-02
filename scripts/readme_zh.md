# DPDK 自动化部署脚本分析与使用指南

本文档提供了对 `scripts` 目录中的脚本进行分析，并指导如何使用这些脚本来自动化 DPDK 的安装、配置和部署过程。

## 脚本分析

这些脚本原本来自 Demikernel 项目，用于自动化 DPDK 的安装、配置和部署过程。以下是各个脚本的功能和用途：

### 1. 配置文件 (`default.yaml` 和 `azure.yaml`)

这些 YAML 文件包含了 DPDK 和网络堆栈的配置参数：

- **网络配置**：IP 地址、MAC 地址和网络接口名称
- **DPDK EAL 初始化参数**：核心掩码、内存通道、PCI 设备地址等
- **TCP/UDP 套接字选项**：保活、延迟、Nagle 算法等
- **网络堆栈配置**：MTU、MSS、巨型帧、校验和卸载等
- **ARP 表配置**：静态 ARP 条目

### 2. 安装脚本

#### `dpdk.sh`
- 下载 DPDK 22.11 版本源码
- 安装必要的依赖（如 pyelftools）
- 使用 meson 和 ninja 构建和安装 DPDK
- 安装到用户的 HOME 目录，而不是系统目录

#### `debian.sh`
- 安装 DPDK 所需的系统依赖
- 包括 RDMA、NUMA、构建工具等

#### `azure.sh`
- 加载特定网卡驱动程序（Mellanox ConnectX-3/4/5）
- 根据检测到的硬件自动选择合适的驱动

### 3. 系统配置脚本

#### `hugepages.sh`
- 配置 1024 个 2MB 大页内存
- 创建并挂载 hugetlbfs 文件系统到 `/mnt/huge`

#### `irq.sh`
- 配置中断亲和性，将所有中断绑定到 CPU 核心 1
- 这有助于隔离网络中断处理，提高性能

#### `config.sh`
- 交互式配置脚本，询问用户网络接口名称和配置文件路径
- 自动检测网络接口的 IP 地址、MAC 地址和 PCI 地址
- 使用这些信息更新配置文件中的占位符

### 4. 部署和运行脚本

#### `deploy.sh`
- 将代码部署到远程主机
- 使用 rsync 同步代码，排除不必要的文件
- 在远程主机上编译代码

#### `run.sh`
- 在远程主机上运行指定的命令
- 使用 sudo 权限执行，并捕获输出

## 如何使用这些脚本自动化 DPDK 部署

基于这些脚本，您可以创建一个完整的自动化部署流程：

### 1. 创建主安装脚本

您可以创建一个主脚本，按照以下顺序调用这些脚本：

```bash
#!/bin/bash

# 安装系统依赖
./scripts/setup/debian.sh  # 对于 Debian/Ubuntu 系统
# 或者为 Fedora 创建一个类似的脚本

# 安装 DPDK
./scripts/setup/dpdk.sh

# 配置大页内存
./scripts/setup/hugepages.sh

# 配置中断
./scripts/setup/irq.sh

# 配置网络
./scripts/setup/config.sh

# 加载特定驱动（如果在 Azure 上）
./scripts/setup/azure.sh
```

### 2. 为 Fedora 创建安装脚本

由于您使用的是 Fedora，您可能需要创建一个类似于 `debian.sh` 的脚本，但使用 `dnf` 而不是 `apt-get`：

```bash
#!/bin/bash

set -e

PACKAGES="rdma-core-devel libmnl-devel gcc-c++ clang numactl-devel pkgconfig python3 python3-pip meson clang-tools-extra"

dnf update
dnf -y install $PACKAGES

# 安装其他可能需要的依赖
pip3 install pyelftools
```

### 3. 修改配置文件

您需要根据您的环境修改 `default.yaml` 或 `azure.yaml` 文件：

- 更新 ENA 网卡的 PCI 地址
- 配置正确的网络接口名称
- 设置适当的 IP 地址和 MAC 地址

### 4. 创建一键部署脚本

最后，您可以创建一个一键部署脚本，自动执行所有步骤：

```bash
#!/bin/bash

# 设置变量
INTERFACE_NAME="ens5"  # 替换为您的 ENA 网卡接口名称
CONFIG_FILE="config.yaml"

# 安装依赖
echo "安装系统依赖..."
./scripts/setup/fedora.sh  # 您需要创建这个脚本

# 安装 DPDK
echo "安装 DPDK..."
./scripts/setup/dpdk.sh

# 配置系统
echo "配置大页内存..."
./scripts/setup/hugepages.sh

echo "配置中断亲和性..."
./scripts/setup/irq.sh

# 自动配置网络
echo "配置网络..."
export PCI_ADDR=`lspci | grep Ethernet | grep -i Amazon | cut -d ' ' -f 1 | head -n 1`
export IPV4_ADDR=`ip addr show $INTERFACE_NAME | grep "inet " | awk '{print $2}' | cut -d/ -f1`
export MAC_ADDR=`ip link show $INTERFACE_NAME | grep "link/ether" | awk '{print $2}'`

# 创建配置文件
cp ./scripts/config/default.yaml $CONFIG_FILE
sed -i "s/abcde/$INTERFACE_NAME/g" $CONFIG_FILE
sed -i "s/XX.XX.XX.XX/$IPV4_ADDR/g" $CONFIG_FILE
sed -i "s/ff:ff:ff:ff:ff:ff/$MAC_ADDR/g" $CONFIG_FILE
sed -i "s/WW:WW.W/$PCI_ADDR/g" $CONFIG_FILE

echo "DPDK 安装和配置完成！"
echo "PCI 地址: $PCI_ADDR"
echo "IP 地址: $IPV4_ADDR"
echo "MAC 地址: $MAC_ADDR"
```

## 针对 ENA 网卡的特殊配置

对于 AWS EC2 实例上的 ENA 网卡，您需要特别注意以下几点：

1. **驱动加载**：确保 ENA 驱动已加载
   ```bash
   modprobe ena
   ```

2. **DPDK EAL 参数**：在配置文件中使用正确的 ENA 设备 PCI 地址
   ```yaml
   dpdk:
     eal_init: ["", "-c", "0xff", "-n", "4", "-a", "PCI_ADDR", "--proc-type=auto"]
   ```

3. **大页内存**：ENA 网卡在高性能场景下可能需要更多的大页内存
   ```bash
   echo 2048 | sudo tee /sys/devices/system/node/node*/hugepages/hugepages-2048kB/nr_hugepages
   ```

## 为 Fedora 系统创建 fedora.sh 脚本

以下是一个为 Fedora 系统创建的安装依赖脚本示例：

```bash
#!/bin/bash

# 设置错误时退出
set -e

# 安装 DPDK 所需的依赖包
PACKAGES="rdma-core-devel libmnl-devel gcc-c++ clang numactl-devel pkgconfig python3 python3-pip meson ninja-build elfutils-libelf-devel libpcap-devel"

echo "更新系统包..."
sudo dnf -y update

echo "安装 DPDK 依赖包..."
sudo dnf -y install $PACKAGES

# 安装 Python 依赖
echo "安装 Python 依赖..."
pip3 install pyelftools

echo "所有依赖安装完成"
```

## 总结

这些脚本提供了一个很好的框架，用于自动化 DPDK 的安装、配置和部署。您可以根据自己的需求进行调整和扩展，特别是针对 Fedora 系统和 ENA 网卡的特殊要求。

主要步骤包括：
1. 安装系统依赖
2. 安装 DPDK
3. 配置大页内存和中断
4. 自动检测和配置网络参数
5. 创建适合您环境的配置文件

通过这种自动化方式，您可以大大简化 DPDK 的部署过程，减少手动配置的错误，并确保一致的环境设置。