use serialport::SerialPort;
use std::io;

fn main() -> io::Result<()> {
  let (mut main, secondary) = serialport::TTYPort::pair()?;
  println!("main[{:?}] secondary[{:?}]", main.name(), secondary.name());
  let mut tick = 0u32;

  loop {
    std::thread::sleep(std::time::Duration::from_secs(3));

    println!("[{tick}] process loop");
    tick += 1;
    let mut buffer = [0u8; 1024];

    match io::Read::read(&mut main, &mut buffer) {
      Ok(amount) => {
        let parsed = std::str::from_utf8(&buffer[0..amount]);
        println!("read {} bytes - {parsed:?}", amount);
      }
      Err(error) if error.kind() == io::ErrorKind::TimedOut => continue,
      Err(error) => {
        println!("unable to read - {error}");
        break;
      }
    }
  }

  Ok(())
}
