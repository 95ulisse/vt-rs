use std::io::prelude::*;
use std::thread::sleep;
use std::time::Duration;
use vt::Console;

fn main() {
    
    // Allocate a new vt
    let console = Console::open().expect("Cannot open console device");
    let original_vt = console.current_vt_number().unwrap();
    let mut vt = console.new_vt_with_minimum_number(7).unwrap();
    println!("Allocated new VT: {}", vt.number());
    
    println!("Switching in 3s...");
    sleep(Duration::from_secs(3));

    // Setup the vt then switch
    vt.clear()
        .and_then(|vt| vt.set_echo(true))
        .and_then(|vt| vt.switch())
        .unwrap();
    
    // Write something
    let n = vt.number();
    writeln!(vt, "Hello world, this is VT {}!", n).unwrap();

    // Blank
    writeln!(vt, "Blanking in 3s...").unwrap();
    sleep(Duration::from_secs(3));
    vt.blank(true).unwrap();
    sleep(Duration::from_secs(3));
    vt.blank(false).unwrap();

    // Switch back
    writeln!(vt, "Example finished. Switching back in 3s...").unwrap();
    sleep(Duration::from_secs(3));
    console.switch_to(original_vt).unwrap();
}