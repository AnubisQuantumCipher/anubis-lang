use anubis_compiler::{frontend::{parse_source,Mode},middle::{ifc2_findings,typecheck_ex}};
fn main() {
 for path in std::env::args().skip(1) {
  let source=std::fs::read_to_string(&path).unwrap();
  let ast=parse_source(&source).unwrap();
  println!("{}\nIFC2: {:?}\nSAFE: {:?}",path,ifc2_findings(&ast,Mode::Safe),typecheck_ex(ast,Mode::Safe,false).map(|_| ())); 
 }
}
