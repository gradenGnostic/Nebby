//! Bounded container inspection; never identifies games from filenames.
use nebby_core::library::{catalog, ImportedGame, RuntimeBackend};
use sha2::{Digest,Sha256};
use std::{fs::File,io::{Read,Seek,SeekFrom},path::Path};

fn read(file:&mut File,offset:u64,size:usize)->Result<Vec<u8>,String>{
    let end=offset.checked_add(size as u64).ok_or("Invalid container range")?;
    if end>file.metadata().map_err(|e|e.to_string())?.len(){return Err("Game dump is truncated".into());}
    let mut data=vec![0;size];
    file.seek(SeekFrom::Start(offset)).and_then(|_|file.read_exact(&mut data)).map_err(|e|e.to_string())?;
    Ok(data)
}
fn u32_at(data:&[u8],offset:usize)->u32{u32::from_le_bytes(data[offset..offset+4].try_into().unwrap())}

pub fn inspect(path:&Path)->Result<ImportedGame,String>{
    let mut file=File::open(path).map_err(|e|format!("Cannot open game dump: {e}"))?;
    let start=read(&mut file,0,0x200)?;
    let base=match &start[0x100..0x104]{
        b"NCCH"=>0,
        b"NCSD"=>u32_at(&start,0x120)as u64*512,
        _=>return Err("Game dump not recognized: use a decrypted CXI or NCSD cartridge dump".into()),
    };
    let h=read(&mut file,base,0x200)?;
    if &h[0x100..0x104]!=b"NCCH"{return Err("Executable partition has no NCCH header".into());}
    if h[0x18f]&4==0{return Err("Encrypted dump: supply your own decrypted game data".into());}
    let title_id=format!("{:016X}",u64::from_le_bytes(h[0x118..0x120].try_into().unwrap()));
    let title=catalog().into_iter().find(|t|t.title_id==title_id).ok_or_else(||format!("Unsupported title ID {title_id}"))?;
    let exefs_offset=u32_at(&h,0x1a0)as u64*512;
    let exefs_size=u32_at(&h,0x1a4)as u64*512;
    let romfs_offset=u32_at(&h,0x1b0)as u64*512;
    let romfs_size=u32_at(&h,0x1b4)as u64*512;
    let content_size=u32_at(&h,0x104)as u64*512;
    if content_size<0x600 || base.checked_add(content_size).is_none_or(|end|end>file.metadata().unwrap().len()){
        return Err("Game partition is truncated".into());
    }
    for (offset,size) in [(exefs_offset,exefs_size),(romfs_offset,romfs_size)]{
        if size==0||offset.checked_add(size).is_none_or(|end|end>content_size){return Err("Invalid ExeFS/RomFS range".into());}
    }
    let ex=read(&mut file,base+0x200,0x400)?;
    let table=read(&mut file,base+exefs_offset,0x200)?;
    let entry=table[..160].chunks_exact(16).find(|entry|&entry[..8]==b".code\0\0\0").ok_or("Executable .code missing")?;
    let offset=u32_at(entry,8)as u64+512;
    let size=u32_at(entry,12)as usize;
    if size==0||size>64*1024*1024||offset.checked_add(size as u64).is_none_or(|end|end>exefs_size){return Err("Invalid executable span".into());}
    let raw_code=read(&mut file,base+exefs_offset+offset,size)?;
    let code=if ex[0x0d]&1!=0{decode_code(&raw_code)?}else{raw_code};
    let hash=format!("{:x}",Sha256::digest(code));
    let (region,revision)=if title.backend==RuntimeBackend::NativeRecomp{
        if title.native_code_sha256.as_deref()!=Some(hash.as_str()){return Err("Unsupported executable revision for this native title profile".into());}
        (title.compatible_region.clone().ok_or("Native region profile missing")?,title.compatible_revision.clone().ok_or("Native revision profile missing")?)
    }else{("Unknown (metadata pending)".into(),"Unknown".into())};
    let game=ImportedGame{title_id,content_path:path.canonicalize().map_err(|e|e.to_string())?.to_string_lossy().into_owned(),region,revision,code_sha256:hash,last_played:None};
    title.validate_import(&game)?;
    Ok(game)
}

// CTR backwards-LZSS: footer describes the compressed suffix and expanded size.
// Checked independently here so native identity always hashes decoded code.
fn decode_code(bytes:&[u8])->Result<Vec<u8>,String>{
    let size=bytes.len();
    if size<8{return Err("Compressed executable footer missing".into());}
    let footer=u32_at(bytes,size-8);
    let padding=(footer>>24)as usize;
    let span=(footer&0xffffff)as usize;
    let expanded=size.checked_add(u32_at(bytes,size-4)as usize).filter(|n|*n<=64*1024*1024).ok_or("Expanded executable too large")?;
    if padding<8||padding>span||span>size{return Err("Invalid executable compression footer".into());}
    let stop=size-span;let mut input=size-padding;let mut output=expanded;
    let mut decoded=vec![0;expanded];decoded[..size].copy_from_slice(bytes);
    while input>stop{
        input-=1;let flags=bytes[input];
        for bit in 0..8{
            if input<=stop{break;}
            if flags&(0x80>>bit)==0{
                input-=1;output=output.checked_sub(1).ok_or("Executable literal overflow")?;
                decoded[output]=bytes[input];
            }else{
                if input-stop<2{return Err("Truncated executable back-reference".into());}
                input-=2;let pair=u16::from_le_bytes([bytes[input],bytes[input+1]])as usize;
                let count=(pair>>12)+3;let distance=(pair&4095)+2;
                if output<count{return Err("Executable run overflow".into());}
                for _ in 0..count{
                    let source=output.checked_add(distance).filter(|n|*n<expanded).ok_or("Executable reference outside output")?;
                    output-=1;decoded[output]=decoded[source];
                }
            }
        }
    }
    if output!=stop{return Err("Executable decompression length mismatch".into());}
    Ok(decoded)
}

#[cfg(test)]mod tests{
    use super::*;
    #[test]fn compressed_code_rejects_invalid_footer(){
        assert!(decode_code(&[]).is_err());
        assert!(decode_code(&[0;8]).is_err());
    }
    #[test]
    #[ignore = "requires user-provided Alpha Sapphire dump"]
    fn local_alpha_native_identity(){
        let input=std::env::var("ALPHA_INPUT").expect("ALPHA_INPUT");
        let game=inspect(Path::new(&input)).unwrap();
        assert_eq!(game.title_id,"000400000011C500");
        assert_eq!(game.revision,"Rev 2");
        assert_eq!(game.code_sha256,"b7f9ce60361f3709ed0ce879658afe10a712f7db060640fa72bba826301b7c16");
    }
    #[test]fn absent_dump_is_an_error(){assert!(inspect(Path::new("/missing/nebby-game.cxi")).is_err());}
    #[test]fn synthetic_experimental_import_and_truncation(){
        let directory=std::env::temp_dir().join(format!("nebby-import-fixture-{}",std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path=directory.join("not-a-game-name.bin");
        let mut bytes=vec![0u8;4096];
        bytes[0x100..0x104].copy_from_slice(b"NCCH");
        bytes[0x104..0x108].copy_from_slice(&8u32.to_le_bytes());
        bytes[0x118..0x120].copy_from_slice(&0x0004000000164800u64.to_le_bytes());
        bytes[0x18f]=4;
        bytes[0x1a0..0x1a4].copy_from_slice(&3u32.to_le_bytes());
        bytes[0x1a4..0x1a8].copy_from_slice(&2u32.to_le_bytes());
        bytes[0x1b0..0x1b4].copy_from_slice(&5u32.to_le_bytes());
        bytes[0x1b4..0x1b8].copy_from_slice(&3u32.to_le_bytes());
        bytes[0x600..0x608].copy_from_slice(b".code\0\0\0");
        bytes[0x60c..0x610].copy_from_slice(&16u32.to_le_bytes());
        std::fs::write(&path,&bytes).unwrap();
        let game=inspect(&path).unwrap();
        assert_eq!(game.title_id,"0004000000164800");
        assert_eq!(catalog().into_iter().find(|t|t.title_id==game.title_id).unwrap().backend,RuntimeBackend::Zakuro);
        std::fs::write(&path,&bytes[..1024]).unwrap();
        assert!(inspect(&path).unwrap_err().contains("truncated"));
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir(&directory).unwrap();
    }
}
