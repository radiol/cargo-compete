use crate::{
    project::{MetadataExt as _, PackageExt as _},
    shell::ColorChoice,
};
use anyhow::{bail, Context, Result};
use clipboard::{ClipboardContext, ClipboardProvider};
use human_size::Size;
use std::{env, fs, iter, path::PathBuf};
use structopt::StructOpt;
use strum::VariantNames as _;

#[derive(StructOpt, Debug)]
#[structopt(
    name = "clip",
    about = "Test (unless --no-test) and copy the source code to clipboard"
)]
pub struct OptCompeteClip {
    #[structopt(long)]
    pub no_test: bool,

    #[structopt(long)]
    pub src: Option<PathBuf>,

    /// Test for only the test cases
    #[structopt(long, value_name("NAME"))]
    pub testcases: Option<Vec<String>>,

    /// Display limit for the test
    #[structopt(long, value_name("SIZE"), default_value("4KiB"))]
    pub display_limit: Size,

    #[structopt(short, long)]
    pub package: Option<String>,

    #[structopt(long)]
    pub manifest_path: Option<PathBuf>,

    #[structopt(long)]
    pub debug: bool,

    #[structopt(long)]
    pub release: bool,

    #[structopt(
        long,
        value_name("WHEN"),
        possible_values(ColorChoice::VARIANTS),
        default_value = "auto"
    )]
    pub color: ColorChoice,

    #[structopt(required_unless("src"))]
    pub name_or_alias: Option<String>,
}

pub fn run(opt: OptCompeteClip, ctx: crate::Context<'_>) -> Result<()> {
    let OptCompeteClip {
        no_test,
        src,
        testcases,
        display_limit,
        package,
        manifest_path,
        debug,
        release,
        color,
        name_or_alias,
    } = opt;

    let crate::Context { cwd, shell, .. } = ctx;
    shell.set_color_choice(color);

    // Step 1: project metadata

    let manifest_path = manifest_path
        .map(|p| Ok(cwd.join(p.strip_prefix(".").unwrap_or(&p))))
        .unwrap_or_else(|| crate::project::locate_project(&cwd))?;

    let metadata = crate::project::cargo_metadata(&manifest_path, &cwd)?;
    let member = metadata.query_for_member(package.as_deref())?;

    // Step 2: bin target
    let bin = if let Some(src) = src {
        let src_path = cwd.join(src.strip_prefix(".").unwrap_or(&src));
        member.bin_target_by_src_path(src_path)?
    } else if let Some(name_or_alias) = name_or_alias.as_deref() {
        let package_metadata = member.read_package_metadata(shell)?;
        let (bin_name, _) = package_metadata.bin_like_by_name_or_alias(name_or_alias)?;
        member.bin_like_target_by_name(bin_name)?
    } else {
        bail!("Either --src or <bin-name> must be provided");
    };

    // Step 3: test (if enabled)
    if !no_test {
        crate::process::process(env::current_exe()?)
            .args(&["compete", "t", "--src"])
            .arg(&bin.src_path)
            .args(&if let Some(testcases) = testcases {
                iter::once("--testcases".into()).chain(testcases).collect()
            } else {
                vec![]
            })
            .args(&["--display-limit", &display_limit.to_string()])
            .args(if debug {
                &["--debug"]
            } else if release {
                &["--release"]
            } else {
                &[]
            })
            .args(&["--manifest-path".as_ref(), member.manifest_path.as_os_str()])
            .args(&["--color", &color.to_string()])
            .cwd(&metadata.workspace_root)
            .exec_with_shell_status(shell)?;
    }

    // Step 4: read file
    let code = fs::read_to_string(&bin.src_path)
        .with_context(|| format!("Failed to read {}", bin.src_path))?;

    // Step 5: copy to clipboard
    let mut ctx: ClipboardContext = ClipboardProvider::new()
        .map_err(|e| anyhow::anyhow!("Failed to initialize clipboard provider: {}", e))?;

    ctx.set_contents(code)
        .map_err(|e| anyhow::anyhow!("Failed to copy to clipboard: {}", e))?;

    shell.status("Success", "Copied source code to clipboard")?;

    Ok(())
}
