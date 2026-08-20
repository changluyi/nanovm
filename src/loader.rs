//! guest 镜像：64 位长模式，直接进入（Firecracker 同款姿势）。
//!
//! VMM 用 KVM_SET_SREGS 把 vCPU 预配置成长模式，跳过实模式→保护模式→长模式
//! 的整个爬梯 —— guest 一睁眼就是 64 位现代世界。
//!
//! 内存布局（guest 物理地址）：
//!   0x1000    PML4（四级页表根）
//!   0x2000    PDPT
//!   0x3000    PD（8 × 2MB 大页，恒等映射 16MB）
//!   0x5000    GDT（null + 64位代码段 + 数据段 + TSS）
//!   0x6000    TSS
//!   0x100000  guest 代码

pub const COM1_PORT: u16 = 0x3f8;

pub const PML4_ADDR: u64 = 0x1000;
pub const PDPT_ADDR: u64 = 0x2000;
pub const PD_ADDR: u64 = 0x3000;
pub const GDT_ADDR: u64 = 0x5000;
pub const TSS_ADDR: u64 = 0x6000;
pub const CODE_ADDR: u64 = 0x100000;
/// 恒等映射的内存总量（8 个 2MB 大页）
pub const MEM_SIZE: usize = 16 * 1024 * 1024;

pub const GUEST_MESSAGE: &str = "hello from nanovm (64-bit)\n";

/// 把页表 / GDT / TSS / 代码写进 guest 内存，返回入口 RIP
pub fn load(mem: &mut [u8]) -> u64 {
    // ── 页表：恒等映射（guest 虚拟地址 == guest 物理地址）──
    // PML4[0] → PDPT
    write_u64(mem, PML4_ADDR, PDPT_ADDR | 0x3); // present | writable
    // PDPT[0] → PD
    write_u64(mem, PDPT_ADDR, PD_ADDR | 0x3);
    // PD[i] → 第 i 个 2MB 大页（PS 位 0x80）
    for i in 0..8u64 {
        write_u64(mem, PD_ADDR + i * 8, (i << 21) | 0x83); // present | writable | huge
    }

    // ── GDT：null / 代码段(L=1) / 数据段 / TSS ──
    write_u64(mem, GDT_ADDR + 0x00, 0);                             // null
    write_u64(mem, GDT_ADDR + 0x08, 0x00AF9A000000FFFF);            // 代码：L=1，64 位
    write_u64(mem, GDT_ADDR + 0x10, 0x00CF93000000FFFF);            // 数据
    write_u64(mem, GDT_ADDR + 0x18, TSS_ADDR | 0x89);               // TSS（简化描述符）
    write_u64(mem, GDT_ADDR + 0x20, 0);

    // ── guest 代码（64 位机器码）──
    // mov dx, 0x3f8 需要 0x66 前缀（64 位下默认操作数是 32 位）
    let mut code = vec![0x66, 0xBA, 0xF8, 0x03];
    for &c in GUEST_MESSAGE.as_bytes() {
        code.extend_from_slice(&[0xB0, c, 0xEE]); // mov al, c; out dx, al
    }
    code.push(0xF4); // hlt
    mem[CODE_ADDR as usize..CODE_ADDR as usize + code.len()].copy_from_slice(&code);

    CODE_ADDR
}

fn write_u64(mem: &mut [u8], guest_phys: u64, val: u64) {
    let off = guest_phys as usize;
    mem[off..off + 8].copy_from_slice(&val.to_le_bytes());
}
