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
use kleya_core::{JsonList, ParsedConfig};

use clap::CommandFactory as _;

use crate::clap_args::{Cli, Cmd, ConfigCmd, TemplateCmd};
use crate::config_loader;
use crate::key_store_fs::FsKeyStore;

pub async fn run(cli: Cli, cancel: tokio_util::sync::CancellationToken) -> kleya_core::Result<()> {
    let config = Arc::new(config_loader::load(cli.config.as_deref())?);
    let effective_provider = resolve_provider(cli.provider.as_deref(), config.provider)?;
    tracing::info!(provider = %effective_provider.as_str(), "using provider");
    let region = cli
        .region
        .clone()
        .unwrap_or_else(|| config.default_region.as_str().to_string());
    // NOTE: `Provider` is `#[non_exhaustive]` so downstream library consumers
    // are not broken by a new variant. Inside this binary we *want* the build
    // to break when a new variant lands without adapter wiring — the wildcard
    // arm below is a deliberate `unreachable!` rather than a silent fallback
    // so anyone adding `Provider::Fly` and forgetting to wire dispatch sees
    // an immediate, loud failure. On nightly this could be promoted to
    // `#[deny(non_exhaustive_omitted_patterns)]` for a true compile error.
    let compute: Arc<dyn CloudCompute> = match effective_provider {
        kleya_core::parsed_config::Provider::Aws => {
            let ec2 = kleya_aws::client::build_ec2_client(&region, None).await;
            let ssm = {
                use aws_config::BehaviorVersion;
                let cfg = aws_config::defaults(BehaviorVersion::latest())
                    .region(aws_sdk_ec2::config::Region::new(region.clone()))
                    .load()
                    .await;
                aws_sdk_ssm::Client::new(&cfg)
            };
            Arc::new(kleya_aws::ec2::AwsEc2 {
                ec2: Arc::new(ec2),
                ssm: Arc::new(ssm),
                region: region.clone(),
            })
        }
        _ => unreachable!(
            "Provider::{effective_provider:?} variant is not wired in dispatch::run; \
             add a match arm that constructs the appropriate CloudCompute adapter"
        ),
    };
    let key_store: Arc<dyn KeyStore> = Arc::new(FsKeyStore::from_parsed(&config)?);
    run_with(cli, config, region, compute, key_store, cancel).await
}

/// Resolve the effective provider given the CLI override (if any) and the
/// value parsed from the user's config. The CLI override wins.
pub(crate) fn resolve_provider(
    cli_provider: Option<&str>,
    config_provider: kleya_core::parsed_config::Provider,
) -> kleya_core::Result<kleya_core::parsed_config::Provider> {
    match cli_provider {
        Some(s) => kleya_core::parsed_config::Provider::parse(s),
        None => Ok(config_provider),
    }
}

#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
pub async fn run_with(
    cli: Cli,
    config: Arc<ParsedConfig>,
    region: String,
    compute: Arc<dyn CloudCompute>,
    key_store: Arc<dyn KeyStore>,
    cancel: tokio_util::sync::CancellationToken,
) -> kleya_core::Result<()> {
    match cli.command {
        Cmd::Template { action } => match action {
            TemplateCmd::Create(args) => {
                let spec = build_template_spec(&config, &args.name, &args).await?;
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
                let spec = build_template_spec(&config, &args.name, &create_args).await?;
                let svc = TemplateService {
                    compute: compute.clone(),
                    config: config.clone(),
                };
                let template_name = kleya_core::model::template::TemplateName::new(&args.name)?;
                let summary = compute
                    .template_get_by_name(&template_name)
                    .await?
                    .ok_or_else(|| kleya_core::Error::TemplateNotFound {
                        name: template_name.clone(),
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
                    let json = serde_json::to_string_pretty(&JsonList::new(&list))
                        .map_err(|e| kleya_core::Error::Io(std::io::Error::other(e)))?;
                    println!("{json}");
                } else {
                    for t in list {
                        println!("{}\t{}\tv{}", t.id, t.name, t.latest_version.0);
                    }
                }
                Ok(())
            }
            TemplateCmd::Delete { name, yes } => {
                if !yes && !confirm(&format!("delete template '{name}'"))? {
                    return Err(kleya_core::Error::UserAborted);
                }
                let template_name = kleya_core::model::template::TemplateName::new(name)?;
                TemplateService { compute, config }
                    .delete_by_name(&template_name)
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
            let connect = args.connect;
            let do_wait = effective_wait_bootstrap(&args);
            let res = svc
                .run(LaunchOpts {
                    template_name: args.template,
                    instance_name: args.name.clone(),
                    instance_type: args.instance_type,
                    market,
                    dry_run: args.dry_run,
                    regenerate_key: args.regenerate_key,
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
                if connect || do_wait {
                    let endpoint =
                        inst.public_dns
                            .clone()
                            .ok_or_else(|| kleya_core::Error::NoPublicDns {
                                instance: inst.id.clone(),
                            })?;
                    crate::ssh_probe::probe_ssh_ready(&endpoint, &inst.id, &cancel).await?;
                    if do_wait {
                        let key = match inst.tags.iter().find(|t| t.key() == "kleya:key") {
                            Some(t) => kleya_core::model::key::KeyName::new(t.value())?,
                            None => config.default_key_name.clone(),
                        };
                        let key_path = key_store.private_path(&key)?;
                        crate::ssh_probe::wait_cloud_init(&key_path, &config.ssh.user, &endpoint)
                            .await?;
                    }
                    if connect {
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
                        let err = Command::new(&plan.command.program)
                            .args(&plan.command.args)
                            .exec();
                        return Err(kleya_core::Error::Io(err));
                    }
                }
            }
            Ok(())
        }
        Cmd::List(args) => {
            let list = ListService { compute }.list_managed().await?;
            if args.json {
                let json = serde_json::to_string_pretty(&JsonList::new(&list))
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
                println!("{}", plan.command.shell_quote());
                return Ok(());
            }
            crate::ssh_probe::probe_ssh_ready(&plan.endpoint, &plan.instance_id, &cancel).await?;
            let err = Command::new(&plan.command.program)
                .args(&plan.command.args)
                .exec();
            Err(kleya_core::Error::Io(err))
        }
        Cmd::Terminate(args) => {
            if !args.yes && !confirm(&format!("terminate '{}'", args.name))? {
                return Err(kleya_core::Error::UserAborted);
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
                let s = toml::to_string_pretty(&config.raw)
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
        Cmd::Completions(args) => {
            let _ = (config, region, compute, key_store, cancel);
            let mut cmd = Cli::command();
            clap_complete::generate(args.shell, &mut cmd, "kleya", &mut std::io::stdout());
            Ok(())
        }
    }
}

async fn build_template_spec(
    config: &Arc<ParsedConfig>,
    name: &str,
    args: &crate::clap_args::TemplateCreateArgs,
) -> kleya_core::Result<kleya_core::model::template::TemplateSpec> {
    use kleya_core::bootstrap::{
        encode::{encode_user_data, encode_user_data_passthrough},
        render::{render, BootstrapVars},
    };
    use kleya_core::model::{
        key::KeyName,
        region::AmiId,
        tag::Tag,
        template::{TemplateName, TemplateSpec},
    };

    let template_name = TemplateName::new(name)?;
    let template_cfg = config.template(&template_name);
    let key_name = match (
        &args.key_name,
        template_cfg.and_then(|t| t.key_name.as_ref()),
    ) {
        (Some(k), _) => KeyName::new(k)?,
        (None, Some(k)) => k.clone(),
        (None, None) => config.default_key_name.clone(),
    };
    let instance_type = args
        .instance_type
        .clone()
        .or_else(|| template_cfg.and_then(|t| t.instance_type.clone()))
        .unwrap_or_else(|| config.default_instance_type.clone());
    let ami_id = match (&args.ami, template_cfg.and_then(|t| t.ami_id.as_ref())) {
        (Some(s), _) => Some(AmiId::new(s)?),
        (None, Some(a)) => Some(a.clone()),
        (None, None) => None,
    };
    let user_data_b64 = if let Some(path) = &args.user_data {
        let bytes = tokio::fs::read(path).await?;
        let raw = String::from_utf8(bytes).map_err(|e| kleya_core::Error::UserDataNotUtf8 {
            reason: e.to_string(),
        })?;
        encode_user_data_passthrough(&raw)?
    } else {
        let vars = BootstrapVars::default_with(kleya_bootstrap_assets::GHOSTTY_TERMINFO);
        encode_user_data(&render(&vars)?)?
    };
    let mut tags = vec![Tag::new("Project", "kleya")?];
    if let Some(t) = template_cfg {
        tags.extend(t.tags.iter().cloned());
    }
    let security_group_ids = template_cfg
        .map(|t| t.security_group_ids.clone())
        .unwrap_or_default();
    let subnet_id = template_cfg.and_then(|t| t.subnet_id.clone());
    Ok(TemplateSpec {
        name: template_name,
        ami_id,
        ami_alias: None,
        instance_type,
        key_name,
        security_group_ids,
        subnet_id,
        market: config.default_market,
        spot_type: config.default_spot_type,
        tags,
        user_data_base64: user_data_b64,
    })
}

pub(crate) fn effective_wait_bootstrap(args: &crate::clap_args::LaunchArgs) -> bool {
    args.wait_bootstrap || (args.connect && !args.no_wait_bootstrap)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clap_args::LaunchArgs;

    fn args(connect: bool, wait: bool, no_wait: bool) -> LaunchArgs {
        LaunchArgs {
            template: None,
            name: None,
            instance_type: None,
            market: None,
            connect,
            wait_bootstrap: wait,
            no_wait_bootstrap: no_wait,
            dry_run: false,
            regenerate_key: false,
        }
    }

    #[test]
    fn effective_wait_explicit_flag_wins() {
        assert!(effective_wait_bootstrap(&args(false, true, false)));
        assert!(effective_wait_bootstrap(&args(false, true, true)));
    }

    #[test]
    fn effective_wait_connect_implies_wait() {
        assert!(effective_wait_bootstrap(&args(true, false, false)));
    }

    #[test]
    fn effective_wait_connect_with_no_wait_skips() {
        assert!(!effective_wait_bootstrap(&args(true, false, true)));
    }

    #[test]
    fn effective_wait_neither_default_false() {
        assert!(!effective_wait_bootstrap(&args(false, false, false)));
    }

    #[test]
    fn resolve_provider_uses_config_when_cli_absent() {
        let got = resolve_provider(None, kleya_core::parsed_config::Provider::Aws)
            .expect("config-only resolve succeeds");
        assert_eq!(got, kleya_core::parsed_config::Provider::Aws);
    }

    #[test]
    fn resolve_provider_accepts_aws_override() {
        let got = resolve_provider(Some("aws"), kleya_core::parsed_config::Provider::Aws)
            .expect("aws override accepted");
        assert_eq!(got, kleya_core::parsed_config::Provider::Aws);
    }

    #[test]
    fn resolve_provider_rejects_fly_override() {
        let err = resolve_provider(Some("fly"), kleya_core::parsed_config::Provider::Aws)
            .expect_err("fly override rejected");
        match err {
            kleya_core::Error::ConfigInvalid { reason } => {
                assert!(reason.contains("fly"), "reason: {reason}");
            }
            other => panic!("expected ConfigInvalid, got {other:?}"),
        }
    }
}
