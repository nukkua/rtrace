pub mod error;
mod launch;
pub use error::Error; 

pub fn run(args: &[String]) -> Result<i32, Error> {
    if args.is_empty() {
        return Err(Error::Usage);
    }

    if args[0] == "-h" {
        println!("{}", Error::Usage);
        return Ok(0);
    }

    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*; 

    #[test]
    fn no_command_is_a_usage_error() {
        let result = run(&[]);
        assert!(matches!(result, Err(Error::Usage)));
    }

    #[test]
    fn a_command_is_accepted() {
        let args = vec!["true".to_string()];
        assert!(matches!(run(&args), Ok(0)));
    }

    #[test]
    fn help_command_accepted() {
        let args = vec!["-h".to_string()];
        assert!(matches!(run(&args), Ok(0)));
    }

    #[test]
    fn converts_errno_into_our_error() {
        fn fails() -> Result<(), Error> {
            Err(nix::errno::Errno::ENOENT)?;
            Ok(())
        }
        assert!(matches!(fails(), Err(Error::Nix(nix::errno::Errno::ENOENT))));
    }


}
