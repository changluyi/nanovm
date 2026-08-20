//! /dev/kvm 的裸 ioctl 封装。
//! 常量按 uapi 头文件 (linux/kvm.h) 手写，是本项目"零依赖"的核心代价与卖点。

use std::fmt;
use std::fs::{File, OpenOptions};
use std::io;
use std::os::unix::io::AsRawFd;
use std::ptr;

const KVMIO: u32 = 0xAE;

// _IO(KVMIO, nr) — 无参数 ioctl
const KVM_GET_API_VERSION: libc::c_ulong = ((KVMIO as libc::c_ulong) << 8) | 0x00;
const KVM_CREATE_VM: libc::c_ulong = ((KVMIO as libc::c_ulong) << 8) | 0x01;
const KVM_GET_VCPU_MMAP_SIZE: libc::c_ulong = ((KVMIO as libc::c_ulong) << 8) | 0x04;
const KVM_CREATE_VCPU: libc::c_ulong = ((KVMIO as libc::c_ulong) << 8) | 0x41;
const KVM_RUN: libc::c_ulong = ((KVMIO as libc::c_ulong) << 8) | 0x80;

// _IOW(KVMIO, 0x46, kvm_userspace_memory_region) — 结构体 32 字节
const KVM_SET_USER_MEMORY_REGION: libc::c_ulong =
    0x4000_0000 | (32 << 16) | ((KVMIO as libc::c_ulong) << 8) | 0x46;

pub const KVM_EXIT_IO: u32 = 2;
pub const KVM_EXIT_HLT: u32 = 5;

#[derive(Debug)]
pub enum KvmError {
    Ioctl(&'static str, io::Error),
    UnsupportedApiVersion(u32),
    Mmap(io::Error),
    UnsupportedExit(u32),
}

impl fmt::Display for KvmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KvmError::Ioctl(what, e) => write!(f, "ioctl {what} 失败: {e}"),
            KvmError::UnsupportedApiVersion(v) => write!(f, "KVM API 版本不支持: {v} (需要 12)"),
            KvmError::Mmap(e) => write!(f, "mmap 失败: {e}"),
            KvmError::UnsupportedExit(r) => write!(f, "未支持的 KVM exit reason: {r}"),
        }
    }
}

impl std::error::Error for KvmError {}


macro_rules! kvm_ioctl {
    ($fd:expr, $nr:expr, $arg:expr, $what:literal) => {
        if unsafe { libc::ioctl($fd.as_raw_fd(), $nr, $arg) } < 0 {
            return Err(KvmError::Ioctl($what, io::Error::last_os_error()));
        }
    };
}

/// 整个虚拟机，对应一个打开的 /dev/kvm fd
pub struct Kvm {
    fd: File,
}

/// 一个虚拟机实例（guest 内存 + vCPU 的容器）
pub struct Vm {
    fd: File,
    /// 持有 guest 物理内存，drop 时 munmap
    regions: Vec<GuestMemRegion>,
    run_mmap_size: usize,
}

/// 一块映射进 guest 物理地址空间的宿主机内存
struct GuestMemRegion {
    addr: *mut u8,
    len: usize,
}

// GuestMemRegion 里是 mmap 的裸指针，跨线程 Send 声明（单线程使用）
unsafe impl Send for GuestMemRegion {}

/// 一个 vCPU —— 一个 fd + 一块共享内存的 kvm_run 结构
pub struct Vcpu {
    fd: File,
    pub run: *mut u8,   // kvm_run 起始地址
    mmap_size: usize,
}

impl Kvm {
    pub fn new() -> Result<Self, KvmError> {
        let fd = OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/kvm")
            .map_err(|e| KvmError::Ioctl("open /dev/kvm", e))?;

        let kvm = Kvm { fd };
        let ver = unsafe { libc::ioctl(kvm.fd.as_raw_fd(), KVM_GET_API_VERSION, 0) };
        if ver != 12 {
            return Err(KvmError::UnsupportedApiVersion(ver as u32));
        }
        Ok(kvm)
    }

    pub fn create_vm(&self) -> Result<Vm, KvmError> {
        let vm_fd = unsafe { libc::ioctl(self.fd.as_raw_fd(), KVM_CREATE_VM, 0) };
        if vm_fd < 0 {
            return Err(KvmError::Ioctl("KVM_CREATE_VM", io::Error::last_os_error()));
        }
        let fd = unsafe { File::from_raw_fd_checked(vm_fd) };
        // vCPU 的 kvm_run 共享内存大小（系统级 ioctl，打在 /dev/kvm fd 上）
        let mmap_size = unsafe { libc::ioctl(self.fd.as_raw_fd(), KVM_GET_VCPU_MMAP_SIZE, 0) };
        if mmap_size <= 0 {
            return Err(KvmError::Ioctl("KVM_GET_VCPU_MMAP_SIZE", io::Error::last_os_error()));
        }
        Ok(Vm { fd, regions: Vec::new(), run_mmap_size: mmap_size as usize })
    }
}

/// 从裸 fd 构造 File 且不关闭原 fd 校验失败的兜底（fd 一定 >= 0，这里不会失败）
trait FromRawFdChecked {
    unsafe fn from_raw_fd_checked(fd: i32) -> File;
}
impl FromRawFdChecked for File {
    unsafe fn from_raw_fd_checked(fd: i32) -> File {
        use std::os::unix::io::FromRawFd;
        File::from_raw_fd(fd)
    }
}

// kvm_run 字段偏移（对照 linux/kvm.h 的 struct kvm_run 布局）
pub mod run_off {
    pub const EXIT_REASON: usize = 8;
    pub const IO_DIRECTION: usize = 32;
    pub const IO_PORT: usize = 34;
    pub const IO_COUNT: usize = 36;
    pub const IO_DATA_OFFSET: usize = 40;
}

pub const KVM_EXIT_IO_OUT: u8 = 1;

// _IOR(KVMIO, 0x81, kvm_regs) — 18 个 u64 = 144 字节
const KVM_GET_REGS: libc::c_ulong =
    0x8000_0000 | (144 << 16) | ((KVMIO as libc::c_ulong) << 8) | 0x81;

/// x86 通用寄存器（KVM_GET_REGS 返回）
#[derive(Debug, Default)]
#[repr(C)]
pub struct Regs {
    pub rax: u64, pub rbx: u64, pub rcx: u64, pub rdx: u64,
    pub rsi: u64, pub rdi: u64, pub rsp: u64, pub rbp: u64,
    pub r8: u64, pub r9: u64, pub r10: u64, pub r11: u64,
    pub r12: u64, pub r13: u64, pub r14: u64, pub r15: u64,
    pub rip: u64, pub rflags: u64,
}

impl Vm {
    /// 分配一页宿主机内存并映射到 guest 物理地址 guest_phys
    pub fn add_memory_region(&mut self, slot: u32, guest_phys: u64, len: usize) -> Result<*mut u8, KvmError> {
        let addr = unsafe {
            libc::mmap(
                ptr::null_mut(),
                len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS | libc::MAP_32BIT,
                -1,
                0,
            )
        };
        if addr == libc::MAP_FAILED {
            return Err(KvmError::Mmap(io::Error::last_os_error()));
        }

        // struct kvm_userspace_memory_region: slot, flags, guest_phys_addr, memory_size, userspace_addr
        #[repr(C)]
        struct Region {
            slot: u32,
            flags: u32,
            guest_phys_addr: u64,
            memory_size: u64,
            userspace_addr: u64,
        }
        let region = Region {
            slot,
            flags: 0,
            guest_phys_addr: guest_phys,
            memory_size: len as u64,
            userspace_addr: addr as u64,
        };

        kvm_ioctl!(
            self.fd,
            KVM_SET_USER_MEMORY_REGION,
            &region as *const Region as libc::c_ulong,
            "KVM_SET_USER_MEMORY_REGION"
        );

        self.regions.push(GuestMemRegion { addr: addr as *mut u8, len });
        Ok(addr as *mut u8)
    }

    pub fn create_vcpu(&self, id: u32) -> Result<Vcpu, KvmError> {
        let vcpu_fd = unsafe { libc::ioctl(self.fd.as_raw_fd(), KVM_CREATE_VCPU, id) };
        if vcpu_fd < 0 {
            return Err(KvmError::Ioctl("KVM_CREATE_VCPU", io::Error::last_os_error()));
        }
        use std::os::unix::io::FromRawFd;
        let fd = unsafe { File::from_raw_fd(vcpu_fd) };

        let mmap_size = self.run_mmap_size;
        let run = unsafe {
            libc::mmap(
                ptr::null_mut(),
                mmap_size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd.as_raw_fd(),
                0,
            )
        };
        if run == libc::MAP_FAILED {
            return Err(KvmError::Mmap(io::Error::last_os_error()));
        }

        Ok(Vcpu { fd, run: run as *mut u8, mmap_size })
    }
}

impl Vcpu {
    /// 执行一次 KVM_RUN，返回后 guest 因某种原因退出了。
    /// EINTR 视为正常打断，重试即可。
    pub fn run(&self) -> Result<(), KvmError> {
        loop {
            let ret = unsafe { libc::ioctl(self.fd.as_raw_fd(), KVM_RUN, 0) };
            if ret >= 0 {
                return Ok(());
            }
            let err = io::Error::last_os_error();
            if err.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(KvmError::Ioctl("KVM_RUN", err));
        }
    }

    /// 读取 kvm_run 中 exit_reason 字段
    pub fn exit_reason(&self) -> u32 {
        unsafe { read_u32(self.run.add(run_off::EXIT_REASON)) }
    }

    /// 读取 KVM_EXIT_IO 的 (direction, port, count, data_offset)
    pub fn io_exit(&self) -> (u8, u16, u32, u64) {
        unsafe {
            let direction = *self.run.add(run_off::IO_DIRECTION);
            let port = read_u16(self.run.add(run_off::IO_PORT));
            let count = read_u32(self.run.add(run_off::IO_COUNT));
            let data_offset = read_u64(self.run.add(run_off::IO_DATA_OFFSET));
            (direction, port, count, data_offset)
        }
    }

    /// KVM_EXIT_IO 的数据区（相对 kvm_run 起始偏移 data_offset）
    pub fn io_data<'a>(&self, offset: u64, count: usize) -> &'a [u8] {
        unsafe { std::slice::from_raw_parts(self.run.add(offset as usize), count) }
    }
}

unsafe fn read_u16(p: *const u8) -> u16 {
    u16::from_ne_bytes([*p, *p.add(1)])
}
unsafe fn read_u32(p: *const u8) -> u32 {
    u32::from_ne_bytes([*p, *p.add(1), *p.add(2), *p.add(3)])
}
unsafe fn read_u64(p: *const u8) -> u64 {
    u64::from_ne_bytes([
        *p, *p.add(1), *p.add(2), *p.add(3),
        *p.add(4), *p.add(5), *p.add(6), *p.add(7),
    ])
}

impl Vcpu {
    /// 读取当前通用寄存器（在 HLT 后调用最有教学价值）
    pub fn get_regs(&self) -> Result<Regs, KvmError> {
        let mut regs = Regs::default();
        if unsafe {
            libc::ioctl(self.fd.as_raw_fd(), KVM_GET_REGS, &mut regs as *mut Regs)
        } < 0 {
            return Err(KvmError::Ioctl("KVM_GET_REGS", io::Error::last_os_error()));
        }
        Ok(regs)
    }
}

impl Drop for GuestMemRegion {
    fn drop(&mut self) {
        unsafe { libc::munmap(self.addr as *mut libc::c_void, self.len) };
    }
}

impl Drop for Vcpu {
    fn drop(&mut self) {
        unsafe { libc::munmap(self.run as *mut libc::c_void, self.mmap_size) };
    }
}
