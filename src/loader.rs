//! guest 镜像：16 位实模式汇编，字节码手写内嵌。
//!
//! 逻辑：dx = 0x3f8 (COM1 端口)；逐字符 out；hlt。
//! 端口写会触发 KVM_EXIT_IO，由 VMM 转发到 stdout。
//!
//! ```asm
//! mov dx, 0x3f8          ; BA F8 03
//! mov al, 'h'            ; B0 68
//! out dx, al             ; EE
//! ...（每个字符 3 字节）
//! hlt                    ; F4
//! ```

pub const COM1_PORT: u16 = 0x3f8;

/// guest 物理地址 0 处的主程序
pub fn guest_code(msg: &str) -> Vec<u8> {
    let mut code = vec![0xBA, 0xF8, 0x03]; // mov dx, 0x3f8
    for &c in msg.as_bytes() {
        code.extend_from_slice(&[0xB0, c, 0xEE]); // mov al, c; out dx, al
    }
    code.push(0xF4); // hlt
    code
}

/// guest 物理地址 0xFFFF_FFF0（复位向量）处的跳转：ljmp 0x0000:0x0000
/// CPU 复位后 CS=0xF000 base=0xFFFF0000 IP=0xFFF0，从这里跳到物理 0
pub const RESET_VECTOR: [u8; 5] = [0xEA, 0x00, 0x00, 0x00, 0x00];

pub const GUEST_MESSAGE: &str = "hello from nanovm\n";
