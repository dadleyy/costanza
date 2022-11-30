//! Big todo. This is currently just a very dumb pseudo-terminal based application that will
//! occasionally write some bytes to the tty and will print out anything that is read from it.
//!
//! This is to help unblock development on the main application that isn't necessarily concerned
//! with the contract between the firmware and the application, but more focused on internal
//! application concerns.

use serialport::SerialPort;
use std::io;
use std::io::Write;

#[derive(Debug)]
enum Message<'a> {
  Command(&'a str),
  Tick(std::time::Instant),
}

#[derive(Default, Debug)]
enum MovementState {
  #[default]
  Idle,
  Moving(std::time::Instant),
}

impl std::fmt::Display for MovementState {
  fn fmt(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
    match self {
      MovementState::Idle => write!(formatter, "Idle"),
      MovementState::Moving(_) => write!(formatter, "Run"),
    }
  }
}

#[derive(Debug, Default)]
struct Machine {
  last_tick: Option<std::time::Instant>,
  movement: MovementState,
  mpos: (f32, f32, f32),
  wpos: (f32, f32, f32),
}

impl Machine {
  fn update(&mut self, message: Message<'_>) -> io::Result<Option<String>> {
    match message {
      Message::Command(utf8_str) => match utf8_str {
        "?" => {
          println!("returning status info");
          let (mx, my, mz) = &self.mpos;
          let (wx, wy, wz) = &self.wpos;
          let status = format!(
            "<{},MPos:{mx:.3},{my:.3},{mz:.3},WPos:{wx:.3},{wy:.3},{wz:.3}>",
            self.movement
          );
          return Ok(Some(status));
        }
        cmd => {
          println!("unknown command ({cmd})");
          let end_at = std::time::Instant::now()
            .checked_add(std::time::Duration::from_secs(2))
            .expect("time problem");
          self.movement = MovementState::Moving(end_at);
          return Ok(Some("ok".into()));
        }
      },
      Message::Tick(time) => {
        self.last_tick = Some(time);

        if let MovementState::Moving(terminate_at) = self.movement {
          if terminate_at < time {
            self.movement = MovementState::Idle;
          }
        }
      }
    }

    Ok(None)
  }
}

fn main() -> io::Result<()> {
  let (mut main, mut secondary) = serialport::TTYPort::pair()?;
  println!("main[{:?}] secondary[{:?}]", main.name(), secondary.name());
  let mut tick = 0u32;
  let mut last_debug = std::time::Instant::now();
  let mut machine = Machine::default();

  secondary.set_exclusive(false)?;

  loop {
    let now = std::time::Instant::now();

    if now.duration_since(last_debug).as_secs() > 5 {
      last_debug = now;
      println!("[{tick}] process loop (ex: {})", main.exclusive());
    }

    tick += 1;
    let mut buffer = [0u8; 1024];

    match io::Read::read(&mut main, &mut buffer) {
      Ok(amount) => {
        let parsed = std::str::from_utf8(&buffer[0..amount]);
        println!("read {} bytes - {parsed:?}", amount);

        if let Ok(valid_utf8) = parsed {
          if let Some(response) = machine.update(Message::Command(valid_utf8.trim_end()))? {
            writeln!(&mut main, "{response}").expect("failed writing response");
          }
        }
      }
      Err(error) if error.kind() == io::ErrorKind::TimedOut => {
        secondary.set_exclusive(false)?;
        machine.update(Message::Tick(std::time::Instant::now()))?;
      }
      Err(error) => {
        println!("unable to read - {error}");
        break;
      }
    }
  }

  eprintln!("closing mock grbl");
  Ok(())
}
