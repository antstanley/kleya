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

pub async fn run(
    cli: Cli,
    cancel: tokio_util::sync::CancellationToken,
) -> kleya_core::Result<()> {
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
    run_with(cli, config, region, compute, key_store, cancel).await
}

#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
pub async fn run_with(
    cli: Cli,
    config: Arc<Config>,
    region: String,
    compute: Arc<dyn CloudCompute>,
    key_store: Arc<dyn KeyStore>,
    cancel: tokio_util::sync::CancellationToken,
) -> kleya_core::Result<()> {
    match cli.command {
        Cmd::Template { action } => match action {
            TemplateCmd::Create(args) => {
                let spec = build_template_spec(&config, &args.name, &args)?;
                TemplateService {
                    compute,
                    config: config.clone(),
                }
                .create(spec)
                .await
                .map(|_| ())
            }
            TemplateCmd::Update(args) => {
                let create_args = crate::clap_args::TemplateCreateArgs {
                    name: args.name.clone(),
                    ami: args.ami,
                    instance_type: args.instance_type,
                    key_name: args.key_name,
                    user_data: args.user_data,
                };
                let spec = build_template_spec(&config, &args.name, &create_args)?;
                let svc = TemplateService {
                    compute: compute.clone(),
                    config: config.clone(),
                };
                let summary = compute
                    .template_get_by_name(&kleya_core::model::template::TemplateName(
                        args.name.clone(),
                    ))
                    .await?
                    .ok_or_else(|| kleya_core::Error::ConfigInvalid {
                        reason: format!("template '{}' not found", args.name),
                    })?;
                svc.update(&summary.id, spec).await.map(|_| ())
            }
            TemplateCmd::List(args) => {
                let svc = TemplateService {
                    compute: compute.clone(),
                    config: config.clone(),
                };
                let list = svc.list().await?;
                if args.json {
                    let json = serde_json::to_string_pretty(&list).map_err(|e| {
                        kleya_core::Error::Io(std::io::Error::other(e))
                    })?;
                    println!("{json}");
                } else {
                    for t in list {
                        println!("{}\t{}\tv{}", t.id.0, t.name.0, t.latest_version.0);
                    }
                }
                Ok(())
            }
            TemplateCmd::Delete { name, yes } => {
                if !yes && !confirm(&format!("delete template '{name}'"))? {
                    return Err(kleya_core::Error::ConfigInvalid {
                        reason: "aborted: pass --yes to confirm".into(),
                    });
                }
                TemplateService { compute, config }
                    .delete_by_name(&kleya_core::model::template::TemplateName(name))
                    .await
            }
        },
        Cmd::Launch(args) => {
            let svc = LaunchService {
                compute: compute.clone(),
                key_store: key_store.clone(),
                id_gen: Arc::new(AdjAnimalIdGen),
                config: config.clone(),
            };
            let market = args.market.map(|m| match m {
                crate::clap_args::Market::OnDemand => {
                    kleya_core::model::market::MarketKind::OnDemand
                }
                crate::clap_args::Market::Spot => kleya_core::model::market::MarketKind::Spot,
            });
            let res = svc
                .run(LaunchOpts {
                    template_name: args.template,
                    instance_name: args.name.clone(),
                    instance_type: args.instance_type,
                    market,
                    dry_run: args.dry_run,
                    cancel: Some(cancel.clone()),
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
                if args.connect || args.wait_bootstrap {
                    let endpoint = inst.public_dns.clone().ok_or_else(|| {
                        kleya_core::Error::ConfigInvalid {
                            reason: format!("instance {} has no public DNS", inst.id.as_str()),
                        }
                    })?;
                    crate::ssh_probe::probe_ssh_ready(&endpoint, &inst.id).await?;
                    if args.wait_bootstrap {
                        let key_name = inst.tags.iter().find(|t| t.key == "kleya:key").map_or_else(
                            || config.keys.default_key_name.clone(),
                            |t| t.value.clone(),
                        );
                        let key = kleya_core::model::key::KeyName::new(key_name)?;
                        let key_path = key_store.private_path(&key)?;
                        crate::ssh_probe::wait_cloud_init(
                            &key_path,
                            &config.ssh.user,
                            &endpoint,
                        )
                        .await?;
                    }
                    if args.connect {
                        let svc = ConnectService {
                            compute,
                            key_store,
                            config,
                            region,
                        };
                        let plan = svc
                            .plan(&ConnectOpts {
                                handle: inst
                                    .name
                                    .as_ref()
                                    .map_or(inst.id.as_str().to_string(), |n| n.as_str().into()),
                                explicit_instance_id: Some(inst.id.as_str().into()),
                                no_tmux: false,
                                tmux_session: None,
                            })
                            .await?;
                        let err = Command::new(&plan.argv[0]).args(&plan.argv[1..]).exec();
                        return Err(kleya_core::Error::Io(err));
                    }
                }
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
            crate::ssh_probe::probe_ssh_ready(&plan.endpoint, &plan.instance_id).await?;
            let err = Command::new(&plan.argv[0]).args(&plan.argv[1..]).exec();
            Err(kleya_core::Error::Io(err))
        }
        Cmd::Terminate(args) => {
            if !args.yes && !confirm(&format!("terminate '{}'", args.name))? {
                return Err(kleya_core::Error::ConfigInvalid {
                    reason: "aborted: pass --yes to confirm".into(),
                });
            }
            TerminateService {
                compute,
                region: region.clone(),
            }
            .terminate_by_handle(&args.name)
            .await
            .map(|_| ())
        }
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

fn build_template_spec(
    config: &Arc<Config>,
    name: &str,
    args: &crate::clap_args::TemplateCreateArgs,
) -> kleya_core::Result<kleya_core::model::template::TemplateSpec> {
    use kleya_core::bootstrap::{
        encode::{encode_user_data, encode_user_data_passthrough},
        render::{render, BootstrapVars},
    };
    use kleya_core::model::{
        key::KeyName,
        market::{MarketKind, SpotType},
        region::AmiId,
        tag::Tag,
        template::{TemplateName, TemplateSpec},
    };
    let template_cfg = config.templates.iter().find(|t| t.name == name);
    let key_name = KeyName::new(
        args.key_name
            .clone()
            .or_else(|| template_cfg.and_then(|t| t.key_name.clone()))
            .unwrap_or_else(|| config.keys.default_key_name.clone()),
    )?;
    let instance_type = args
        .instance_type
        .clone()
        .or_else(|| template_cfg.and_then(|t| t.instance_type.clone()))
        .unwrap_or_else(|| config.defaults.instance_type.clone());
    let ami_id = args
        .ami
        .clone()
        .or_else(|| template_cfg.and_then(|t| t.ami_id.clone()))
        .map(AmiId);
    let user_data_b64 = if let Some(path) = &args.user_data {
        let bytes = std::fs::read(path)?;
        let raw = String::from_utf8(bytes).map_err(|e| kleya_core::Error::ConfigInvalid {
            reason: format!("user-data not utf-8: {e}"),
        })?;
        encode_user_data_passthrough(&raw)?
    } else {
        let vars = BootstrapVars::default_with(kleya_bootstrap_assets::GHOSTTY_TERMINFO);
        encode_user_data(&render(&vars)?)?
    };
    let mut tags = vec![Tag::new("Project", "kleya")?];
    if let Some(t) = template_cfg {
        for tag in &t.tags {
            tags.push(Tag::new(&tag.key, &tag.value)?);
        }
    }
    let market = match config.defaults.market.as_str() {
        "on-demand" => MarketKind::OnDemand,
        _ => MarketKind::Spot,
    };
    let spot_type = match config.defaults.spot_type.as_str() {
        "persistent" => SpotType::Persistent,
        _ => SpotType::OneTime,
    };
    Ok(TemplateSpec {
        name: TemplateName(name.to_string()),
        ami_id,
        ami_alias: None,
        instance_type,
        key_name,
        security_group_ids: template_cfg
            .and_then(|t| t.security_group_ids.clone())
            .unwrap_or_default()
            .into_iter()
            .map(kleya_core::model::region::SecurityGroupId)
            .collect(),
        subnet_id: template_cfg
            .and_then(|t| t.subnet_id.clone())
            .map(kleya_core::model::region::SubnetId),
        market,
        spot_type,
        tags,
        user_data_base64: user_data_b64,
    })
}

fn confirm(action: &str) -> kleya_core::Result<bool> {
    use std::io::Write as _;
    eprint!("{action}? [y/N] ");
    std::io::stderr().flush()?;
    let mut buf = String::new();
    std::io::stdin().read_line(&mut buf)?;
    let answer = buf.trim().to_ascii_lowercase();
    Ok(answer == "y" || answer == "yes")
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
