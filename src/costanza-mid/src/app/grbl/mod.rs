use std::io;

#[derive(Debug)]
pub enum Command {
  Status,
}

impl std::fmt::Display for Command {
  fn fmt(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
    match self {
      Self::Status => writeln!(formatter, "?"),
    }
  }
}

#[derive(Debug, Clone, Copy)]
pub enum MachineState {
  Run,
  Idle,
  Home,
  Alarm,
}

impl std::str::FromStr for MachineState {
  type Err = io::Error;

  fn from_str(input: &str) -> Result<Self, Self::Err> {
    match input {
      "Idle" => Ok(Self::Idle),
      "Run" => Ok(Self::Run),
      "Home" => Ok(Self::Home),
      "Alarm" => Ok(Self::Alarm),
      unknown => Err(io::Error::new(
        io::ErrorKind::Other,
        format!("bad machine state - {unknown}"),
      )),
    }
  }
}

#[derive(Debug, Clone, Copy)]
pub struct MachinePosition {
  #[allow(dead_code)]
  x: f32,
  #[allow(dead_code)]
  y: f32,
  #[allow(dead_code)]
  z: f32,
}

#[derive(Debug)]
pub enum Response {
  Ok,
  Status(MachineState, MachinePosition),
}

impl std::str::FromStr for Response {
  type Err = io::Error;

  fn from_str(input: &str) -> Result<Self, Self::Err> {
    match input {
      "ok" => Ok(Self::Ok),
      status if status.starts_with('<') => {
        let chars = status.chars().skip(1);
        let state = chars
          .take_while(|c| *c != ',')
          .collect::<String>()
          .parse::<MachineState>()?;

        tracing::info!("parsed machine state - {state:?} (from {status})");

        match &status.split(',').skip(1).collect::<Vec<&str>>()[..] {
          [header, raw_y, raw_z, _, _, _] if header.starts_with("MPos:") => {
            let x = header
              .trim_start_matches("MPos:")
              .parse::<f32>()
              .map_err(|error| io::Error::new(io::ErrorKind::Other, format!("bad machine pos - {error}")))?;
            let y = raw_y
              .parse::<f32>()
              .map_err(|error| io::Error::new(io::ErrorKind::Other, format!("bad machine pos - {error}")))?;
            let z = raw_z
              .parse::<f32>()
              .map_err(|error| io::Error::new(io::ErrorKind::Other, format!("bad machine pos - {error}")))?;
            tracing::info!("found machine pos ({x}, {y}, {z})");
            Ok(Self::Status(state, MachinePosition { x, y, z }))
          }
          unknown => Err(io::Error::new(
            io::ErrorKind::Other,
            format!("bad status bits - '{unknown:?}'"),
          )),
        }
      }
      other => Err(io::Error::new(
        io::ErrorKind::Other,
        format!("unknown grbl response - '{other}'"),
      )),
    }
  }
}
