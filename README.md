# nanovm

**一个 ~400 行、零依赖（仅 libc）的 KVM microVM。**

小到一天能读完的 Firecracker —— 用最少的代码回答"microVM 到底是什么"。

## 快速开始

```bash
cargo run --release
# hello from nanovm
# [nanovm] guest 执行完毕，耗时 326.193µs

cargo run --release -- echo   # 交互式 echo 机器：敲什么回什么，Ctrl-D 退出
```

要求：Linux + `/dev/kvm`（WSL2 开启嵌套虚拟化也可）。

## 它做了什么

1. 打开 `/dev/kvm`，创建 VM
2. mmap 两页内存映射进 guest 物理地址空间（代码页 @ 物理地址 0，复位向量页 @ 0xFFFF_F000）
3. 放入手写机器码：16 位实模式汇编，往 COM1 端口（0x3f8）逐字符 `out`，最后 `hlt`；echo 模式则 `in`/`out` 循环
4. 创建 vCPU（一个线程 + 一块共享内存 kvm_run），进入 `KVM_RUN` 循环
5. guest 每次端口写触发 `KVM_EXIT_IO`，VMM 转发到 stdout；端口读则 VMM 阻塞读 stdin 喂给 guest；`hlt` 则干净退出

CPU 复位后从 0xFFFF_FFF0 取指，那里放了一条 `ljmp 0:0` 跳到主程序 —— 复位向量技巧，省掉整个固件引导链。

## 与 Firecracker 对比

| | Firecracker | nanovm |
|---|---|---|
| 代码量 | ~10 万行 | **~400 行** |
| 依赖 | ~100 crates | **1（libc）** |
| 二进制 | 3.4 MB | **371 KB** |
| guest | Linux 直启 | 实模式汇编 |
| 设备 | virtio blk/net/rng/serial | 端口 IO |
| 用途 | 生产（AWS Lambda/Fargate） | 教学 |

Firecracker 源码中与本项目的对应关系：`vstate/vcpu.rs` ↔ `src/vcpu.rs`，
`vmm/src/builder.rs` ↔ `src/main.rs`。

## 扩展方向

本项目的边界就是“最小可运行的 KVM VM”。想继续深入，可以自己加：16550 UART 模拟、Linux 内核直启（boot protocol）、virtio-mmio 设备 —— 每一步都能在 Firecracker 源码里找到对照实现。

## License

MIT
