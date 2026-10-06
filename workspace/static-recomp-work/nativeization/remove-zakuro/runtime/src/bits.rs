//! ARM bitfield operations. No runtime dependencies.
#[inline(always)]pub const fn bit(value:u32,n:u32)->bool{(value>>n)&1!=0}
#[inline(always)]pub const fn bits(value:u32,lo:u32,hi:u32)->u32{(value>>lo)&(u32::MAX>>(31-(hi-lo)))}
#[inline(always)]pub const fn sign_extend(value:u32,n:u32)->i32{let shift=32-n;((value<<shift)as i32)>>shift}
