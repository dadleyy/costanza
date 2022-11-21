use serialport::SerialPort;
use std::io;
use std::io::Write;

fn main() -> io::Result<()> {
  let (mut main, secondary) = serialport::TTYPort::pair()?;
  println!("main[{:?}] secondary[{:?}]", main.name(), secondary.name());
  let mut tick = 0u32;
  let mut last_status = std::time::Instant::now();
  let mut last_debug = std::time::Instant::now();

  loop {
    std::thread::sleep(std::time::Duration::from_millis(10));
    let now = std::time::Instant::now();

    if now.duration_since(last_debug).as_secs() > 5 {
      last_debug = now;
      println!("[{tick}] process loop");
    }

    tick += 1;
    let mut buffer = [0u8; 1024];

    if now.duration_since(last_status).as_secs() > 2 {
      last_status = now;

      if let Err(error) = writeln!(&mut main, "status") {
        println!("unable to write status - {error}");
        break;
      }
    }

    match io::Read::read(&mut main, &mut buffer) {
      Ok(amount) => {
        let parsed = std::str::from_utf8(&buffer[0..amount]);
        println!("read {} bytes - {parsed:?}", amount);
        writeln!(&mut main, "ok").expect("failed writing response");
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
