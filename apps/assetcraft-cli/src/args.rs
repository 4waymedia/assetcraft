//! Hand-rolled argument parsing (no clap: five subcommands with a handful of flags each).
//!
//! Rules: `--flag` switches, `--key value` or `--key=value` options, `-h`/`--help` anywhere,
//! `--` ends flags (so a file named `-x.png` works), and a lone `-` is a positional.

/// The flags one subcommand accepts.
pub struct Spec {
    /// Boolean switches, e.g. `"--json"`.
    pub switches: &'static [&'static str],
    /// Options that take a value, e.g. `"--sort"`.
    pub values: &'static [&'static str],
}

/// Parsed arguments for one subcommand.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Parsed {
    /// Positional arguments in order.
    pub positionals: Vec<String>,
    /// Switches that were given.
    pub switches: Vec<&'static str>,
    /// Options that were given (last one wins).
    pub values: Vec<(&'static str, String)>,
    /// `-h` / `--help` was given.
    pub help: bool,
}

impl Parsed {
    /// `true` if `switch` was given.
    pub fn has(&self, switch: &str) -> bool {
        self.switches.contains(&switch)
    }

    /// The value of option `name`, if given.
    pub fn value(&self, name: &str) -> Option<&str> {
        self.values.iter().rev().find(|(n, _)| *n == name).map(|(_, v)| v.as_str())
    }
}

/// Parse `args` (everything after the subcommand name) against `spec`.
pub fn parse(args: &[String], spec: &Spec) -> Result<Parsed, String> {
    let mut out = Parsed::default();
    let mut iter = args.iter();
    let mut flags_done = false;
    while let Some(arg) = iter.next() {
        if flags_done || arg == "-" || !arg.starts_with('-') {
            out.positionals.push(arg.clone());
            continue;
        }
        if arg == "--" {
            flags_done = true;
            continue;
        }
        if arg == "-h" || arg == "--help" {
            out.help = true;
            continue;
        }
        let (name, inline) = match arg.split_once('=') {
            Some((n, v)) => (n, Some(v)),
            None => (arg.as_str(), None),
        };
        if let Some(switch) = spec.switches.iter().find(|s| **s == name) {
            if inline.is_some() {
                return Err(format!("`{name}` takes no value"));
            }
            if !out.switches.contains(switch) {
                out.switches.push(switch);
            }
        } else if let Some(option) = spec.values.iter().find(|s| **s == name) {
            let value = match inline {
                Some(v) => v.to_string(),
                None => iter.next().cloned().ok_or_else(|| format!("`{name}` needs a value"))?,
            };
            out.values.push((option, value));
        } else {
            return Err(format!("unknown flag `{name}`"));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC: Spec = Spec { switches: &["--json", "--desc"], values: &["--sort", "--out"] };

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn switches_values_and_positionals_in_any_order() {
        let p = parse(&args(&["--json", "dir one", "--sort", "size", "--desc", "--out=x.png"]), &SPEC).unwrap();
        assert_eq!(p.positionals, ["dir one"]);
        assert!(p.has("--json") && p.has("--desc"));
        assert_eq!(p.value("--sort"), Some("size"));
        assert_eq!(p.value("--out"), Some("x.png"));
        assert!(!p.help);
    }

    #[test]
    fn last_value_wins_and_switches_dedupe() {
        let p = parse(&args(&["--sort", "name", "--sort", "date", "--json", "--json"]), &SPEC).unwrap();
        assert_eq!(p.value("--sort"), Some("date"));
        assert_eq!(p.switches, ["--json"]);
    }

    #[test]
    fn help_double_dash_and_lone_dash() {
        let p = parse(&args(&["-h", "-", "--", "--json", "-x.png"]), &SPEC).unwrap();
        assert!(p.help);
        assert!(!p.has("--json"));
        assert_eq!(p.positionals, ["-", "--json", "-x.png"]);
    }

    #[test]
    fn errors() {
        assert_eq!(parse(&args(&["--bogus"]), &SPEC).unwrap_err(), "unknown flag `--bogus`");
        assert_eq!(parse(&args(&["-j"]), &SPEC).unwrap_err(), "unknown flag `-j`");
        assert_eq!(parse(&args(&["--sort"]), &SPEC).unwrap_err(), "`--sort` needs a value");
        assert_eq!(parse(&args(&["--json=1"]), &SPEC).unwrap_err(), "`--json` takes no value");
    }

    #[test]
    fn unicode_positionals_pass_through() {
        let p = parse(&args(&["Réaper ünïcode name.png"]), &SPEC).unwrap();
        assert_eq!(p.positionals, ["Réaper ünïcode name.png"]);
    }
}
