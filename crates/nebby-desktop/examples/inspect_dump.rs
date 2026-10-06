fn main(){
    let path=std::env::args_os().nth(1).expect("usage: inspect_dump <owned dump>");
    match nebby_desktop::import::inspect(std::path::Path::new(&path)){
        Ok(game)=>println!("{} {} {} {}",game.title_id,game.region,game.revision,game.code_sha256),
        Err(error)=>{eprintln!("{error}");std::process::exit(1);}
    }
}
