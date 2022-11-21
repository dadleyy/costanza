use serialport::SerialPort;
use std::io;

fn main() -> io::Result<()> {
  let (mut main, secondary) = serialport::TTYPort::pair()?;
  println!("main[{:?}] secondary[{:?}]", main.name(), secondary.name());
  let mut tick = 0u32;
  let mut last_status = std::time::Instant::now();

  loop {
    std::thread::sleep(std::time::Duration::from_secs(3));
    let now = std::time::Instant::now();

    println!("[{tick}] process loop");
    tick += 1;
    let mut buffer = [0u8; 1024];

    if now.duration_since(last_status).as_secs() > 5 {
      last_status = now;

      if let Err(error) = io::Write::write(&mut main, b"status...\n") {
        println!("unable to write status - {error}");
        break;
      }
    }

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
