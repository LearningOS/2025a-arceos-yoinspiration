#![cfg_attr(feature = "axstd", no_std)]
#![cfg_attr(feature = "axstd", no_main)]
#![feature(asm_const)]
#![feature(riscv_ext_intrinsics)]

#[cfg(feature = "axstd")]
extern crate axstd as std;
extern crate alloc;
#[macro_use]
extern crate axlog;

mod task;
mod vcpu;
mod regs;
mod csrs;
mod sbi;
mod loader;

use vcpu::VmCpuRegisters;
use riscv::register::{scause, sstatus, stval, mhartid, htinst};
use csrs::defs::hstatus;
use tock_registers::LocalRegisterCopy;
use csrs::{RiscvCsrTrait, CSR};
use vcpu::_run_guest;
use sbi::SbiMessage;
use loader::load_vm_image;
use axhal::mem::PhysAddr;
use crate::regs::GprIndex::{A0, A1};

#[cfg_attr(feature = "axstd", no_mangle)]
fn main() {
    ax_println!("Hypervisor ...");

    // A new address space for vm.
    let mut uspace = axmm::new_user_aspace().unwrap();

    // Load vm binary file into address space.
    let entry = match load_vm_image("/sbin/skernel2", &mut uspace) {
        Ok(e) => e,
        Err(e) => panic!("Cannot load app! {:?}", e),
    };

    // Setup context to prepare to enter guest mode.
    let mut ctx = VmCpuRegisters::default();
    let guest_page_table_root = uspace.page_table_root();
    
    // Debug: Verify entry address has mapping in page table
    let gpa = match uspace.page_table().query(entry.into()) {
        Ok((paddr, flags, level)) => {
            ax_println!("Entry {:#x} mapped to GPA {:#x}, flags: {:?}, level: {:?}", 
                       entry, paddr, flags, level);
            paddr
        },
        Err(e) => {
            ax_println!("WARNING: Entry {:#x} not found in page table: {:?}", entry, e);
            panic!("Cannot find entry address in page table");
        }
    };
    
    // For HGATP to work, we need GPA -> HPA mapping in the page table.
    // In simple implementation, we assume GPA = HPA (identity mapping).
    // Add identity mapping for GPA -> HPA in the same page table.
    // This is needed because HGATP uses the page table to translate GPA to HPA.
    use axhal::paging::MappingFlags;
    use axhal::mem::{PAGE_SIZE_4K, VirtAddr};
    
    // Calculate the size of the loaded binary and create identity mapping
    let aligned_size = (4096 + PAGE_SIZE_4K - 1) & !(PAGE_SIZE_4K - 1); // 4096 is the binary size
    let gpa_vaddr = VirtAddr::from(gpa.as_usize());
    ax_println!("Adding identity mapping for GPA {:#x} -> HPA {:#x}, size: {:#x}", 
                gpa, gpa, aligned_size);
    uspace.map_linear(gpa_vaddr, gpa, aligned_size, 
                      MappingFlags::READ|MappingFlags::WRITE|MappingFlags::EXECUTE|MappingFlags::USER)
        .unwrap_or_else(|e| panic!("Failed to add identity mapping for GPA {:#x}: {:?}", gpa, e));
    
    // Verify the identity mapping was created correctly
    match uspace.page_table().query(gpa_vaddr) {
        Ok((hpa, flags, level)) => {
            ax_println!("Identity mapping verified: GPA {:#x} -> HPA {:#x}, flags: {:?}", 
                       gpa, hpa, flags);
            if hpa != gpa {
                panic!("Identity mapping mismatch: GPA {:#x} -> HPA {:#x}", gpa, hpa);
            }
        },
        Err(e) => {
            panic!("Failed to verify identity mapping for GPA {:#x}: {:?}", gpa, e);
        }
    }
    
    prepare_guest_context(&mut ctx, entry, guest_page_table_root);

    // Setup pagetable for 2nd address mapping (HGATP for guest physical to host physical).
    // In simple implementation, we use the same page table with identity mapping (GPA = HPA).
    prepare_vm_pgtable(guest_page_table_root);

    // Kick off vm and wait for it to exit.
    while !run_guest(&mut ctx) {
    }

    panic!("Hypervisor ok!");
}

fn prepare_vm_pgtable(ept_root: PhysAddr) {
    let hgatp = 8usize << 60 | usize::from(ept_root) >> 12;
    unsafe {
        core::arch::asm!(
            "csrw hgatp, {hgatp}",
            hgatp = in(reg) hgatp,
        );
        core::arch::riscv64::hfence_gvma_all();
    }
}

fn run_guest(ctx: &mut VmCpuRegisters) -> bool {
    unsafe {
        _run_guest(ctx);
    }

    vmexit_handler(ctx)
}

#[allow(unreachable_code)]
fn vmexit_handler(ctx: &mut VmCpuRegisters) -> bool {
    use scause::{Exception, Trap};

    let scause = scause::read();
    match scause.cause() {
        Trap::Exception(Exception::VirtualSupervisorEnvCall) => {
            let sbi_msg = SbiMessage::from_regs(ctx.guest_regs.gprs.a_regs()).ok();
            ax_println!("VmExit Reason: VSuperEcall: {:?}", sbi_msg);
            if let Some(msg) = sbi_msg {
                match msg {
                    SbiMessage::Reset(_) => {
                        let a0 = ctx.guest_regs.gprs.reg(A0);
                        let a1 = ctx.guest_regs.gprs.reg(A1);
                        ax_println!("a0 = {:#x}, a1 = {:#x}", a0, a1);
                        assert_eq!(a0, 0x6688);
                        assert_eq!(a1, 0x1234);
                        ax_println!("Shutdown vm normally!");
                        return true;
                    },
                    _ => todo!(),
                }
            } else {
                panic!("bad sbi message! ");
            }
        },
        Trap::Exception(Exception::IllegalInstruction) => {
            // In RISC-V virtualization, the instruction encoding is in htinst, not stval
            let inst = htinst::read();
            ax_println!("Illegal instruction: {:#x} at sepc: {:#x}", inst, ctx.guest_regs.sepc);
            
            // Check if it's csrr a1, mhartid (0xf14025f3)
            // csrr rd, csr encoding: | csr[11:0] | rd[4:0] | opcode |
            // 0xf14025f3 = csrr a1(11), mhartid(0xf14)
            if inst == 0xf14025f3 {
                // Simulate: set a1 = mhartid (typically 0)
                let hartid = mhartid::read();
                ctx.guest_regs.gprs.set_reg(A1, hartid);
                ax_println!("Emulated csrr a1, mhartid: a1 = {:#x}", hartid);
                // Skip the instruction
                ctx.guest_regs.sepc += 4;
                return false;
            }
            
            panic!("Bad instruction: {:#x} sepc: {:#x}",
                inst,
                ctx.guest_regs.sepc
            );
        },
        Trap::Exception(Exception::InstructionGuestPageFault) => {
            let gva = stval::read();
            ax_println!("InstructionGuestPageFault: GVA={:#x} sepc={:#x}", gva, ctx.guest_regs.sepc);
            panic!("InstructionGuestPageFault: GVA={:#x} sepc={:#x} - Page table may not have correct mapping",
                gva,
                ctx.guest_regs.sepc
            );
        },
        Trap::Exception(Exception::LoadGuestPageFault) => {
            panic!("LoadGuestPageFault: stval{:#x} sepc: {:#x}",
                stval::read(),
                ctx.guest_regs.sepc
            );
        },
        _ => {
            panic!(
                "Unhandled trap: {:?}, sepc: {:#x}, stval: {:#x}",
                scause.cause(),
                ctx.guest_regs.sepc,
                stval::read()
            );
        }
    }
    false
}

fn prepare_guest_context(ctx: &mut VmCpuRegisters, entry: usize, guest_page_table_root: PhysAddr) {
    // Set hstatus
    let mut hstatus = LocalRegisterCopy::<usize, hstatus::Register>::new(
        riscv::register::hstatus::read().bits(),
    );
    // Set Guest bit in order to return to guest mode.
    hstatus.modify(hstatus::spv::Guest);
    // Set SPVP bit in order to accessing VS-mode memory from HS-mode.
    hstatus.modify(hstatus::spvp::Supervisor);
    CSR.hstatus.write_value(hstatus.get());
    ctx.guest_regs.hstatus = hstatus.get();

    // Set sstatus in guest mode.
    let mut sstatus = sstatus::read();
    sstatus.set_spp(sstatus::SPP::Supervisor);
    ctx.guest_regs.sstatus = sstatus.bits();
    // Return to entry to start vm.
    ctx.guest_regs.sepc = entry;
    
    // Set guest VSATP (Virtual Supervisor Address Translation and Protection)
    // This maps guest virtual addresses to guest physical addresses.
    // For simple binary loading, we use the same page table root.
    // VSATP format: MODE (bits 63-60) | PPN (bits 43-0)
    // MODE=8 for Sv39, MODE=9 for Sv48
    // Note: VSATP PPN is in host physical address, not guest physical address
    let vsatp = 8usize << 60 | usize::from(guest_page_table_root) >> 12;
    ctx.vs_csrs.vsatp = vsatp;
    ax_println!("VSATP set to: {:#x}, page_table_root: PA:{:#x}", vsatp, guest_page_table_root);
    
    // Debug: VSATP setup complete
}
