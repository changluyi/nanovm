# nanovm 架构

## 模块结构

```mermaid
flowchart TB
    subgraph main["main.rs — 装配线 (41行)"]
        A[Kvm::new<br/>打开 /dev/kvm] --> B[create_vm]
        B --> C[add_memory_region ×2<br/>物理 0x0 + 0xFFFF_F000]
        C --> D[loader 写入 guest 代码<br/>+ 复位向量]
        D --> E[create_vcpu 0]
        E --> F[vcpu::run_loop]
    end

    subgraph kvm["kvm.rs — 裸 ioctl 封装 (272行)"]
        K1[Kvm] --> K2[Vm] --> K3[Vcpu]
        K3 -.->|mmap 共享内存| R[(kvm_run)]
    end

    subgraph vcpu["vcpu.rs — 主循环 (29行)"]
        V1[KVM_RUN] --> V2{exit_reason?}
        V2 -->|IO| V3[端口 0x3f8 输出<br/>→ stdout]
        V2 -->|HLT| V4[干净退出]
        V2 -->|其他| V5[报错]
    end

    subgraph loader["loader.rs — guest 镜像 (30行)"]
        L1[实模式机器码<br/>mov dx,0x3f8 / out / hlt]
        L2[复位向量<br/>ljmp 0:0]
    end

    F --> V1
    C -.映射.-> loader
```

## 一次完整执行的时序

```mermaid
sequenceDiagram
    participant U as 用户
    participant V as VMM (Rust)
    participant K as KVM (内核)
    participant G as Guest (实模式)

    U->>V: cargo run
    V->>K: open /dev/kvm
    V->>K: KVM_CREATE_VM
    V->>K: KVM_SET_USER_MEMORY_REGION ×2
    Note over V,K: mmap 两页 = guest 的全部"物理内存"
    V->>K: KVM_CREATE_VCPU
    V->>K: mmap kvm_run (共享内存)

    loop 每个 KVM_RUN
        V->>K: ioctl(KVM_RUN)
        K->>G: CPU 进入 guest 模式
        Note over G: 取指执行真实机器码
        G-->>K: out 0x3f8, al (端口写)
        K-->>V: KVM_EXIT_IO
        Note over V: 从 kvm_run 读数据<br/>打印到 stdout
    end

    V->>K: ioctl(KVM_RUN)
    K->>G: 进入 guest 模式
    G-->>K: hlt
    K-->>V: KVM_EXIT_HLT
    V-->>U: 进程干净退出 (exit 0)
```

## Guest 内存布局

```mermaid
flowchart LR
    subgraph guest["guest 物理地址空间"]
        subgraph page0["页0 @ 0x0 (4KB)"]
            code["guest 代码 (57字节)<br/>mov dx,0x3f8<br/>mov al,'h'; out dx,al ×18<br/>hlt"]
        end
        subgraph pageF["页1 @ 0xFFFF_F000 (4KB)"]
            reset["@ 0xFFFF_FFF0:<br/>ljmp 0x0000:0x0000"]
        end
    end
    reset -->|"CPU 复位后从这里跳转"| code
```

## 与 Firecracker 的模块对照

| nanovm | Firecracker | 职责 |
|---|---|---|
| `src/kvm.rs` | `src/vmm/src/vstate/kvm.rs` | KVM ioctl 封装 |
| `src/vcpu.rs` | `src/vmm/src/vstate/vcpu.rs` | vCPU 线程 + exit 分发 |
| `src/main.rs` | `src/vmm/src/builder.rs` | VM 装配 |
| `src/loader.rs` | `src/vmm/src/builder.rs` (内核加载) | guest 镜像放置 |
