//! nanovm —— 一个 ~400 行的教学级 KVM microVM。
//!
//! 与 Firecracker 相同的地基（KVM），删到只剩最小可运行集：
//! 实模式 guest + 端口 IO + HLT。用来回答"microVM 到底是什么"。

mod kvm;
mod loader;
mod vcpu;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 打开 KVM，确认 API 版本
    let kvm = kvm::Kvm::new()?;

    // 2. 建 VM
    let mut vm = kvm.create_vm()?;

    // 3. guest 物理内存：一页放代码（物理 0），一页放复位向量（物理 0xFFFF_F000）
    let code_page = vm.add_memory_region(0, 0x0, 4096)?;
    let reset_page = vm.add_memory_region(1, 0xFFFF_F000, 4096)?;

    // 4. 加载 guest
    let code = loader::guest_code(loader::GUEST_MESSAGE);
    unsafe {
        std::ptr::copy_nonoverlapping(code.as_ptr(), code_page, code.len());
        // 复位向量在页内偏移 0xFF0
        std::ptr::copy_nonoverlapping(
            loader::RESET_VECTOR.as_ptr(),
            reset_page.add(0xFF0),
            loader::RESET_VECTOR.len(),
        );
    }

    // 5. 建 vCPU 并跑（kvm_run 共享内存映射在 create_vcpu 里完成）
    let vcpu = vm.create_vcpu(0)?;

    let start = std::time::Instant::now();
    let stats = vcpu::run_loop(&vcpu)?;
    eprintln!(
        "[nanovm] guest 执行完毕：{} 次 VM exit（其中 IO {} 次），耗时 {:?}",
        stats.total_exits, stats.io_exits, start.elapsed()
    );

    Ok(())
}
