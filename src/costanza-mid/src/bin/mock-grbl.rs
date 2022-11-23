//! Big todo. This is currently just a very dumb pseudo-terminal based application that will
//! occasionally write some bytes to the tty and will print out anything that is read from it.
//!
//! This is to help unblock development on the main application that isn't necessarily concerned
//! with the contract between the firmware and the application, but more focused on internal
//! application concerns.

use serialport::SerialPort;
use std::io;
use std::io::Write;

fn main() -> io::Result<()> {
  loop {
    let (mut main, secondary) = serialport::TTYPort::pair()?;
    println!("main[{:?}] secondary[{:?}]", main.name(), secondary.name());
    let mut tick = 0u32;
    let mut last_status = std::time::Instant::now();
    let mut last_debug = std::time::Instant::now();
    let mut last_read = std::time::Instant::now();

    main.set_exclusive(false)?;

    'inner: loop {
      std::thread::sleep(std::time::Duration::from_millis(10));
      let now = std::time::Instant::now();

      if now.duration_since(last_debug).as_secs() > 5 {
        last_debug = now;
        println!("[{tick}] process loop");
      }

      // TODO: attempts to connect to a previously disconnected pseudo terminal return a "device
      // or resource busy" error. To replicate:
      //
      // 1. run this "mock"
      // 2. run the `costanza-m` application configured to the pty location printed from the mock
      // 3. terminate the `costanza-m` application
      // 4. run the `costanza-m` application again
      //    ^-- The application will fail to connect.
      //
      // There might be a problem with the way the pty is being created above.
      if now.duration_since(last_read).as_secs() > 10 {
        eprintln!("restarting connection");
        break 'inner;
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
          last_read = std::time::Instant::now();
        }
        Err(error) if error.kind() == io::ErrorKind::TimedOut => continue,
        Err(error) => {
          println!("unable to read - {error}");
          break;
        }
      }
    }
  }
}
