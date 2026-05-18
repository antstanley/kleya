use aws_config::BehaviorVersion;
use aws_sdk_ec2::config::Region;
use aws_sdk_ec2::Client as Ec2Client;

pub async fn build_ec2_client(region: &str, endpoint_url: Option<&str>) -> Ec2Client {
    let mut loader =
        aws_config::defaults(BehaviorVersion::latest()).region(Region::new(region.to_string()));
    if let Some(url) = endpoint_url {
        loader = loader.endpoint_url(url);
    }
    let cfg = loader.load().await;
    Ec2Client::new(&cfg)
}
