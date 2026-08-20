//! 单元测试：不依赖 KVM 的纯逻辑验证（页表/GDT 布局）。

#[path = "../src/loader.rs"]
mod loader;

#[test]
fn page_tables_identity_map_16mb() {
    let mut mem = vec![0u8; 0x400000]; // 4MB 足够覆盖页表/GDT 区域
    loader::load(&mut mem);

    let read_u64 = |addr: u64| {
        let off = addr as usize;
        u64::from_le_bytes(mem[off..off + 8].try_into().unwrap())
    };

    // PML4[0] → PDPT，present + writable
    assert_eq!(read_u64(loader::PML4_ADDR), loader::PDPT_ADDR | 0x3);
    // PDPT[0] → PD
    assert_eq!(read_u64(loader::PDPT_ADDR), loader::PD_ADDR | 0x3);
    // PD[0] → 物理 0 的 2MB 大页：present + writable + huge(PS位)
    assert_eq!(read_u64(loader::PD_ADDR), 0x83);
    // PD[1] → 物理 2MB
    assert_eq!(read_u64(loader::PD_ADDR + 8), (1 << 21) | 0x83);
}

#[test]
fn gdt_has_64bit_code_segment() {
    let mut mem = vec![0u8; 0x400000];
    loader::load(&mut mem);

    let code_desc = u64::from_le_bytes(
        mem[0x5008..0x5010].try_into().unwrap()
    );
    // 0x00AF9A000000FFFF：L=1（64位）、present、exec/read
    assert_eq!(code_desc, 0x00AF9A000000FFFF);
}

#[test]
fn guest_code_is_written_at_entry() {
    let mut mem = vec![0u8; 0x400000];
    let entry = loader::load(&mut mem);
    assert_eq!(entry, loader::CODE_ADDR);

    // 0x66 0xBA = mov dx, imm16（64 位下的前缀形式）
    assert_eq!(&mem[entry as usize..entry as usize + 4], &[0x66, 0xBA, 0xF8, 0x03]);
    // 最后一条是 hlt
    let len = 4 + loader::GUEST_MESSAGE.len() * 3 + 1;
    assert_eq!(mem[entry as usize + len - 1], 0xF4);
}
