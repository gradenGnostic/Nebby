//! Checked guest spans, independent of host pointer width/platform.
pub fn page_span(address:u32,size:u32)->Option<std::ops::Range<usize>>{
    let first=(address>>12)as usize;
    if size==0{return Some(first..first);}
    let last=address.checked_add(size-1)?;
    Some(first..(last>>12)as usize+1)
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn unaligned_span_covers_last_page(){assert_eq!(page_span(0xfff,4),Some(0..2));assert_eq!(page_span(0x1000,4096),Some(1..2));}
    #[test]fn top_page_and_overflow(){assert_eq!(page_span(0xfffff000,4096),Some(0xfffff..0x100000));assert_eq!(page_span(0xffffffff,2),None);assert_eq!(page_span(7,0),Some(0..0));}
}
pub mod owner;
pub mod address_space;
