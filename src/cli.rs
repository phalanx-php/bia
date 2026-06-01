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

Inline command strings should use expressions, direct dory() calls, or Dory's
1-2 character bare-variable shorthand. Use a script file or heredoc for PHP
code that needs normal $ variables or longer variable names."#
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
