use clap::Parser;

#[derive(Parser)]
#[command(
    name = "bia",
    version,
    about = "Phalanx Scripting Environment",
    after_help = r#"Quick start:
  bia -r 'bia()->dump("hello")'
  bia -r 'bia()->dump(1 + 1)'
  bia -r 'a = 3; bia()->dump(a * 3)'
  bia run script.php
  bia doctor

Slim vars: inline -r code auto-adds $ to 1-2 lowercase letter variables.
  OK:   a, z, vv, ab        (1-2 lowercase letters)
  Bad:  aaa, _a, A1         (too long, underscore, uppercase)
  Bad:  if, do, fn, or, as  (reserved keywords)

For normal PHP variables, use a heredoc:
  bia -r "<<'PHP'
  $longName = fetchData();
  bia()->dump($longName);
  PHP"
"#
)]
pub struct BiaCli {
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
