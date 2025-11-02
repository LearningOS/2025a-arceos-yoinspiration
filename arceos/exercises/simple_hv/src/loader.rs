use std::io::{self, Read};
use std::fs::File;
use alloc::vec::Vec;
use axhal::paging::MappingFlags;
use axhal::mem::{PAGE_SIZE_4K, phys_to_virt};
use axmm::AddrSpace;
use crate::VM_ENTRY;

pub fn load_vm_image(fname: &str, uspace: &mut AddrSpace) -> io::Result<()> {
    // Read entire file
    let mut file = File::open(fname)?;
    let mut data = Vec::new();
    file.read_to_end(&mut data)?;
    
    let file_size = data.len();
    ax_println!("app: {}, size: {} bytes", fname, file_size);

    // Align size to page boundary
    let aligned_size = (file_size + PAGE_SIZE_4K - 1) & !(PAGE_SIZE_4K - 1);
    
    uspace.map_alloc(VM_ENTRY.into(), aligned_size, MappingFlags::READ|MappingFlags::WRITE|MappingFlags::EXECUTE|MappingFlags::USER, true).unwrap();

    let (paddr, _, _) = uspace
        .page_table()
        .query(VM_ENTRY.into())
        .unwrap_or_else(|_| panic!("Mapping failed for segment: {:#x}", VM_ENTRY));

    ax_println!("paddr: {:#x}", paddr);

    // Zero out the page first
    unsafe {
        core::ptr::write_bytes(
            phys_to_virt(paddr).as_mut_ptr(),
            0,
            aligned_size,
        );
    }
    
    // Copy file data
    unsafe {
        core::ptr::copy_nonoverlapping(
            data.as_ptr(),
            phys_to_virt(paddr).as_mut_ptr(),
            file_size,
        );
    }

    Ok(())
}
