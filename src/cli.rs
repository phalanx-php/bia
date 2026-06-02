use clap::Parser;

#[derive(Parser)]
#[command(
    name = "dory",
    version,
    about = "Phalanx Scripting Environment",
    after_help = r#"Quick start:
  dory -r 'dory()->dump("hello")'
  dory -r 'dory()->dump(1 + 1)'
  dory -r 'a = 3; dory()->dump(a * 3)'
  dory run script.php
  dory doctor

Slim vars: inline -r code auto-adds $ to 1-2 lowercase letter variables.
  OK:   a, z, vv, ab        (1-2 lowercase letters)
  Bad:  aaa, _a, A1         (too long, underscore, uppercase)
  Bad:  if, do, fn, or, as  (reserved keywords)

For normal PHP variables, use a heredoc:
  dory -r "$(cat <<'PHP'
  $longName = fetchData();
  dory()->dump($longName);
  PHP
  )"
"#
)]
pub struct DoryCli {
    #[arg(short, long)]
    pub verbose: bool,

    #[arg(
        short = 'r',
        visible_short_alias = 'e',
        long = "run-code",
        value_name = "CODE"
    )]
    pub code: Option<String>,

    #[arg(allow_hyphen_values = true)]
    pub args: Vec<String>,
}
