use std::os::unix::process::CommandExt as _;
use std::process::Command;
use std::sync::Arc;

use kleya_core::commands::{
    connect::{ConnectOpts, ConnectService},
    launch::{LaunchOpts, LaunchService},
    list::ListService,
    template::TemplateService,
    terminate::TerminateService,
};
use kleya_core::ports::cloud_compute::CloudCompute;
use kleya_core::ports::id_gen::AdjAnimalIdGen;
use kleya_core::ports::key_store::KeyStore;
use kleya_core::Config;

use crate::clap_args::{Cli, Cmd, ConfigCmd, TemplateCmd};
use crate::config_loader;
use crate::key_store_fs::FsKeyStore;

pub async fn run(cli: Cli) -> kleya_core::Result<()> {
    let config = Arc::new(config_loader::load(cli.config.as_deref())?);
    let region = cli
        .region
        .clone()
        .unwrap_or_else(|| config.default_region.clone());
    let ec2 = kleya_aws::client::build_ec2_client(&region, None).await;
    let ssm = {
        use aws_config::BehaviorVersion;
        let cfg = aws_config::defaults(BehaviorVersion::latest())
            .region(aws_sdk_ec2::config::Region::new(region.clone()))
            .load()
            .await;
        aws_sdk_ssm::Client::new(&cfg)
    };
    let compute: Arc<dyn CloudCompute> = Arc::new(kleya_aws::ec2::AwsEc2 {
        ec2: Arc::new(ec2),
        ssm: Arc::new(ssm),
        region: region.clone(),
    });
    let key_store: Arc<dyn KeyStore> = Arc::new(FsKeyStore::from_config(&config.keys)?);
    run_with(cli, config, region, compute, key_store).await
}

#[allow(clippy::too_many_lines)]
pub async fn run_with(
    cli: Cli,
    config: Arc<Config>,
    region: String,
    compute: Arc<dyn CloudCompute>,
    key_store: Arc<dyn KeyStore>,
) -> kleya_core::Result<()> {
    match cli.command {
        Cmd::Template { action } => match action {
            TemplateCmd::Create(_) | TemplateCmd::Update(_) => {
                Err(kleya_core::Error::ConfigInvalid {
                    reason: "template create/update via CLI deferred to launch flow".into(),
                })
            }
            TemplateCmd::List => {
                let svc = TemplateService {
                    compute: compute.clone(),
                    config: config.clone(),
                };
                for t in svc.list().await? {
                    println!("{}\t{}\tv{}", t.id.0, t.name.0, t.latest_version.0);
                }
                Ok(())
            }
            TemplateCmd::Delete { name, .. } => {
                TemplateService { compute, config }
                    .delete_by_name(&kleya_core::model::template::TemplateName(name))
                    .await
            }
        },
        Cmd::Launch(args) => {
            let svc = LaunchService {
                compute,
                key_store,
                id_gen: Arc::new(AdjAnimalIdGen),
                config,
                bootstrap_tpl: kleya_bootstrap_assets::SETUP_TEMPLATE,
                ghostty_tinfo: kleya_bootstrap_assets::GHOSTTY_TERMINFO,
            };
            let res = svc
                .run(LaunchOpts {
                    template_name: args.template,
                    instance_name: args.name,
                    dry_run: args.dry_run,
                })
                .await?;
            if let Some(inst) = &res {
                println!(
                    "launched: id={} name={} dns={}",
                    inst.id.as_str(),
                    inst.name
                        .as_ref()
                        .map_or("-", kleya_core::model::instance::InstanceName::as_str),
                    inst.public_dns.as_deref().unwrap_or("-"),
                );
            }
            Ok(())
        }
        Cmd::List(args) => {
            let list = ListService { compute }.list_managed().await?;
            if args.json {
                let json = serde_json::to_string_pretty(&list)
                    .map_err(|e| kleya_core::Error::Io(std::io::Error::other(e)))?;
                println!("{json}");
            } else {
                for i in list {
                    println!(
                        "{}\t{}\t{:?}\t{}",
                        i.id.as_str(),
                        i.name
                            .as_ref()
                            .map_or("-", kleya_core::model::instance::InstanceName::as_str),
                        i.state,
                        i.public_dns.unwrap_or_else(|| "-".into()),
                    );
                }
            }
            Ok(())
        }
        Cmd::Connect(args) => {
            let svc = ConnectService {
                compute,
                key_store,
                config,
                region,
            };
            let plan = svc
                .plan(&ConnectOpts {
                    handle: args.name,
                    explicit_instance_id: args.instance_id,
                    no_tmux: args.no_tmux,
                    tmux_session: args.tmux_session,
                })
                .await?;
            if args.print {
                println!("{}", shell_quote(&plan.argv));
                return Ok(());
            }
            let err = Command::new(&plan.argv[0]).args(&plan.argv[1..]).exec();
            Err(kleya_core::Error::Io(err))
        }
        Cmd::Terminate(args) => TerminateService {
            compute,
            region: region.clone(),
        }
        .terminate_by_handle(&args.name)
        .await
        .map(|_| ()),
        Cmd::Config { action } => match action {
            ConfigCmd::Show => {
                let s = toml::to_string_pretty(&*config)
                    .map_err(|e| kleya_core::Error::Io(std::io::Error::other(e)))?;
                println!("{s}");
                Ok(())
            }
            ConfigCmd::Path => {
                println!(
                    "{}",
                    config_loader::resolved_path(cli.config.as_deref())
                        .unwrap_or_else(|| "<defaults; no file loaded>".into())
                );
                Ok(())
            }
        },
    }
}

fn shell_quote(argv: &[String]) -> String {
    argv.iter()
        .map(|s| {
            if s.chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_/.@=:".contains(c))
            {
                s.clone()
            } else {
                format!("'{}'", s.replace('\'', r"'\''"))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
