use std::io::{self, Read};
use std::io::SeekFrom;
use std::io::Seek;
use std::fs::File;
use alloc::vec::Vec;
use alloc::vec;
use axhal::paging::MappingFlags;
use axhal::mem::{PAGE_SIZE_4K, VirtAddr, MemoryAddr};
use axmm::AddrSpace;

use elf::abi::{PT_INTERP, PT_LOAD};
use elf::endian::AnyEndian;
use elf::parse::ParseAt;
use elf::segment::ProgramHeader;
use elf::segment::SegmentTable;
use elf::ElfBytes;

const ELF_HEAD_BUF_SIZE: usize = 256;
const BINARY_LOAD_ADDR: usize = 0x8020_0000;
const ELF_MAGIC: [u8; 4] = [0x7f, 0x45, 0x4c, 0x46]; // "\x7fELF"

pub fn load_vm_image(fname: &str, uspace: &mut AddrSpace) -> io::Result<usize> {
    let mut file = File::open(fname)?;
    
    // Check if it's an ELF file by reading the magic number
    let mut magic_buf = [0u8; 4];
    file.read_exact(&mut magic_buf)?;
    file.seek(SeekFrom::Start(0))?; // Reset to beginning
    
    if magic_buf == ELF_MAGIC {
        // Load as ELF file
        ax_println!("app: {}, ELF format", fname);
        load_elf_file(&mut file, uspace)
    } else {
        // Load as raw binary file
        ax_println!("app: {}, binary format", fname);
        load_binary_file(&mut file, uspace, fname)
    }
}

fn load_elf_file(file: &mut File, uspace: &mut AddrSpace) -> io::Result<usize> {
    let (phdrs, entry, _, _) = load_elf_phdrs(file)?;

    for phdr in &phdrs {
        ax_println!(
            "phdr: offset: {:#X}=>{:#X} size: {:#X}=>{:#X}",
            phdr.p_offset, phdr.p_vaddr, phdr.p_filesz, phdr.p_memsz
        );

        let vaddr = VirtAddr::from(phdr.p_vaddr as usize).align_down_4k();
        let vaddr_end = VirtAddr::from((phdr.p_vaddr+phdr.p_memsz) as usize)
            .align_up_4k();

        ax_println!("VA:{:#x} - VA:{:#x}", vaddr, vaddr_end);
        uspace.map_alloc(vaddr, vaddr_end-vaddr, MappingFlags::READ|MappingFlags::WRITE|MappingFlags::EXECUTE|MappingFlags::USER, true)?;

        let mut data = vec![0u8; phdr.p_memsz as usize];
        file.seek(SeekFrom::Start(phdr.p_offset))?;

        let filesz = phdr.p_filesz as usize;
        let mut index = 0;
        while index < filesz {
            let n = file.read(&mut data[index..filesz])?;
            index += n;
        }
        assert_eq!(index, filesz);
        uspace.write(VirtAddr::from(phdr.p_vaddr as usize), &data)?;
    }

    Ok(entry)
}

fn load_binary_file(file: &mut File, uspace: &mut AddrSpace, fname: &str) -> io::Result<usize> {
    // Read entire file
    let mut data = Vec::new();
    file.read_to_end(&mut data)?;
    
    let file_size = data.len();
    ax_println!("app: {}, size: {} bytes", fname, file_size);
    
    // Debug: Print first few bytes
    if file_size >= 4 {
        ax_println!("First 4 bytes: {:#02x} {:#02x} {:#02x} {:#02x}", 
                    data[0], data[1], data[2], data[3]);
    }

    // Align size to page boundary
    let aligned_size = (file_size + PAGE_SIZE_4K - 1) & !(PAGE_SIZE_4K - 1);
    
    uspace.map_alloc(BINARY_LOAD_ADDR.into(), aligned_size, MappingFlags::READ|MappingFlags::WRITE|MappingFlags::EXECUTE|MappingFlags::USER, true)?;

    // Write file data using virtual address (consistent with ELF loading)
    uspace.write(BINARY_LOAD_ADDR.into(), &data)?;
    
    // Debug: Verify the write by reading back
    let mut verify_buf = vec![0u8; 4.min(file_size)];
    uspace.read(BINARY_LOAD_ADDR.into(), &mut verify_buf)?;
    ax_println!("Verify read back first 4 bytes: {:#02x} {:#02x} {:#02x} {:#02x}",
                verify_buf[0], verify_buf[1], verify_buf[2], verify_buf[3]);

    Ok(BINARY_LOAD_ADDR)
}

fn load_elf_phdrs(file: &mut File) -> io::Result<(Vec<ProgramHeader>, usize, usize, usize)> {
    let mut buf: [u8; ELF_HEAD_BUF_SIZE] = [0; ELF_HEAD_BUF_SIZE];
    file.read(&mut buf)?;

    let ehdr = ElfBytes::<AnyEndian>::parse_elf_header(&buf[..]).unwrap();
    ax_println!("entry: {:#x}", ehdr.e_entry);

    let phnum = ehdr.e_phnum as usize;
    // Validate phentsize before trying to read the table so that we can error early for corrupted files
    let entsize = ProgramHeader::validate_entsize(ehdr.class, ehdr.e_phentsize as usize).unwrap();
    let size = entsize.checked_mul(phnum).unwrap();
    assert!(size > 0 && size <= PAGE_SIZE_4K);
    let phoff = ehdr.e_phoff;
    let mut buf = alloc::vec![0u8; size];
    let _ = file.seek(SeekFrom::Start(phoff));
    file.read(&mut buf)?;
    let phdrs = SegmentTable::new(ehdr.endianness, ehdr.class, &buf[..]);

    let phdrs: Vec<ProgramHeader> = phdrs
        .iter()
        .filter(|phdr| phdr.p_type == PT_LOAD || phdr.p_type == PT_INTERP)
        .collect();
    Ok((phdrs, ehdr.e_entry as usize, ehdr.e_phoff as usize, ehdr.e_phnum as usize))
}
