// from Anubis
use risc0_zkvm::guest::env; fn main(){let x:u32=env::read();let y=x*6;env::commit(&y);}