use clap::Parser;

#[derive(Parser)]
#[command(
    name = "dory",
    version,
    about = "Phalanx Scripting Environment",
    after_help = "Quick start:\n  dory -r 'dory()->dump(\"hello\")'\n  dory run script.php\n  dory doctor",
)]
pub struct DoryCli {
    #[arg(short, long)]
    pub verbose: bool,

    #[arg(short = 'r', visible_short_alias = 'e', long = "run-code", value_name = "CODE")]
    pub code: Option<String>,

    #[arg(allow_hyphen_values = true)]
    pub args: Vec<String>,
}
