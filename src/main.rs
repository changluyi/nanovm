//! nanovm —— 一个 ~500 行的教学级 KVM microVM。
//!
//! 与 Firecracker 相同的地基（KVM）和相同的现代姿势：
//! VMM 用 KVM_SET_SREGS/SET_REGS 把 vCPU 直接配置进 64 位长模式，
//! guest 跳过实模式爬梯，一睁眼就是现代世界。

mod kvm;
mod loader;
mod vcpu;

use kvm::{Dtable, Regs, Segment, Sregs};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 打开 KVM，确认 API 版本
    let kvm = kvm::Kvm::new()?;

    // 2. 建 VM
    let mut vm = kvm.create_vm()?;

    // 3. guest 物理内存：一块 16MB（页表/GDT/代码都在里面）
    let mem = vm.add_memory_region(0, 0x0, loader::MEM_SIZE)?;

    // 4. 加载 guest（页表 + GDT + 64 位代码），拿到入口地址
    let entry = unsafe { loader::load(std::slice::from_raw_parts_mut(mem, loader::MEM_SIZE)) };

    // 5. 建 vCPU 并配置成 64 位长模式
    let vcpu = vm.create_vcpu(0)?;
    vcpu.set_sregs(&long_mode_sregs())?;
    vcpu.set_regs(&Regs {
        rip: entry,
        rflags: 0x2, // bit1 必须为 1
        ..Default::default()
    })?;

    // 6. 跑！
    let start = std::time::Instant::now();
    let stats = vcpu::run_loop(&vcpu)?;
    eprintln!(
        "[nanovm] guest 执行完毕：{} 次 VM exit（其中 IO {} 次），耗时 {:?}",
        stats.total_exits, stats.io_exits, start.elapsed()
    );

    Ok(())
}

/// 构造"已开启分页的 64 位长模式"的 CPU 现场
fn long_mode_sregs() -> Sregs {
    let code_seg = Segment {
        base: 0,
        limit: 0xFFFFF,
        selector: 0x08,
        type_: 11,        // code: exec/read/accessed
        present: 1,
        s: 1,             // 代码/数据段（非系统段）
        l: 1,             // ← L 位：64 位模式的关键
        g: 1,             // 粒度 4KB
        ..Default::default()
    };
    let data_seg = Segment {
        base: 0,
        limit: 0xFFFFF,
        selector: 0x10,
        type_: 3,         // data: read/write/accessed
        present: 1,
        s: 1,
        db: 1,
        g: 1,
        ..Default::default()
    };
    let mut sregs = Sregs {
        cs: code_seg,
        ds: data_seg.clone(),
        es: data_seg.clone(),
        ss: data_seg.clone(),
        ..Default::default()
    };
    // fs/gs/ldt 保持 unusable（长模式数据段基本是摆设）
    // TR：给一个最小可用 TSS
    sregs.tr = Segment {
        base: loader::TSS_ADDR,
        limit: 0x67,
        selector: 0x18,
        type_: 11, // busy 64-bit TSS
        present: 1,
        ..Default::default()
    };
    sregs.gdt = Dtable { base: loader::GDT_ADDR, limit: 0x2F, ..Default::default() };
    sregs.cr0 = 0x8000_0031; // PG | ET | NE | PE —— 分页 + 保护已开
    sregs.cr3 = loader::PML4_ADDR; // 页表根
    sregs.cr4 = 0x20; // PAE —— 长模式必需
    sregs.efer = 0x500; // LME | LMA —— 长模式开启
    sregs
}
