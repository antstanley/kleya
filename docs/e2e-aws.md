# kleya e2e against real AWS

End-to-end tests that drive `AwsEc2` against a live AWS account. They
are **opt-in** — never run on CI by default — and live behind the
`KLEYA_TEST_E2E=1` environment gate. Default to running them in a
dedicated sandbox account; never against production.

## Required IAM permissions

The test principal needs the following actions on `Resource: "*"`
unless noted. Reads are unconditional; mutating EC2 calls may be
optionally narrowed with tag conditions (see the next section). The
SSM resource is a narrow public-parameter ARN.

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Sid": "Ec2Read",
      "Effect": "Allow",
      "Action": [
        "ec2:DescribeInstances",
        "ec2:DescribeLaunchTemplates",
        "ec2:DescribeLaunchTemplateVersions",
        "ec2:DescribeKeyPairs",
        "ec2:DescribeSecurityGroups",
        "ec2:DescribeVpcs",
        "ec2:DescribeSubnets"
      ],
      "Resource": "*"
    },
    {
      "Sid": "Ec2Mutate",
      "Effect": "Allow",
      "Action": [
        "ec2:CreateLaunchTemplate",
        "ec2:CreateLaunchTemplateVersion",
        "ec2:ModifyLaunchTemplate",
        "ec2:DeleteLaunchTemplate",
        "ec2:RunInstances",
        "ec2:TerminateInstances",
        "ec2:StartInstances",
        "ec2:StopInstances",
        "ec2:CreateTags",
        "ec2:ImportKeyPair",
        "ec2:DeleteKeyPair",
        "ec2:CreateSecurityGroup",
        "ec2:AuthorizeSecurityGroupIngress"
      ],
      "Resource": "*"
    },
    {
      "Sid": "SsmAmiAlias",
      "Effect": "Allow",
      "Action": "ssm:GetParameter",
      "Resource": "arn:aws:ssm:*::parameter/aws/service/ami-amazon-linux-latest/*"
    }
  ]
}
```

`StartInstances` / `StopInstances` are listed for the forward-looking
`kleya start` / `kleya stop` commands — they have no consumer in
`AwsEc2` today, but adding them now means the policy does not need to
be re-rolled when those land.

### Optional: tag-scoped mutation for multi-tenant sandboxes

If the sandbox account is shared, add the following `Condition` blocks
so kleya can only ever mutate resources it owns:

- `aws:RequestTag/kleya:managed = "true"` on `ec2:RunInstances`,
  `ec2:CreateLaunchTemplate`, `ec2:CreateTags`.
- `aws:ResourceTag/kleya:managed = "true"` on `ec2:TerminateInstances`,
  `ec2:DeleteLaunchTemplate`, `ec2:DeleteKeyPair`.

This makes accidental termination of non-kleya resources structurally
impossible. The `AwsEc2` adapter already tags every launched instance,
template, and keypair with `kleya:managed=true` (see
[docs/specs/04-provider-port.md](specs/04-provider-port.md#runinstances-tagging)).

## Environment

Set in the test shell:

```
KLEYA_TEST_E2E=1          # opt-in gate
AWS_REGION=eu-west-1          # or another region with a default VPC
AWS_PROFILE=kleya-sandbox     # or AWS_ACCESS_KEY_ID/SECRET via env
```

The default VPC must exist in the chosen region; the tests rely on
`CloudCompute::resolve_default_subnet`, which filters VPCs by
`isDefault=true` and picks the lexicographically first AZ's subnet.
Accounts created after December 2013 get a default VPC automatically;
older accounts may need one created (`aws ec2 create-default-vpc`).

## Running

```
KLEYA_TEST_E2E=1 cargo nextest run -p kleya-aws --run-ignored all
```

Tests are `#[ignore]` until the env gate flips them on, mirroring the
existing Floci pattern (`KLEYA_TEST_FLOCI=1`). Whole-workspace runs
ignore them; the gate must be set explicitly.

## Cleanup

Every test must clean up its own resources, even on panic:

- Launched instances → `instance_terminate` in test teardown / `Drop`.
- Created launch templates → `template_delete`.
- Imported keypairs → `keypair_delete`.
- Created security groups → leave the `kleya-default` SG in place
  (idempotent across runs; `ensure_default_security_group` treats an
  existing same-named SG as success).

We tag every resource with `kleya:e2e-run = <ISO-8601 UTC>` (in
addition to the standard `kleya:managed=true`) so a janitor can find
orphans by timestamp.

### Janitor (manual run)

If a crash left orphans behind, enumerate them with the AWS CLI:

```bash
aws ec2 describe-instances \
  --filters 'Name=tag:kleya:managed,Values=true' \
            'Name=instance-state-name,Values=pending,running,stopping,stopped' \
  --query 'Reservations[].Instances[].[InstanceId,LaunchTime,Tags[?Key==`kleya:e2e-run`].Value|[0]]' \
  --output table
```

Resources older than 1 hour with a `kleya:e2e-run` tag are safe to
terminate. Equivalent queries exist for launch templates and key pairs:

```bash
aws ec2 describe-launch-templates \
  --filters 'Name=tag:kleya:managed,Values=true' \
  --query 'LaunchTemplates[].[LaunchTemplateId,LaunchTemplateName,CreateTime]' \
  --output table

aws ec2 describe-key-pairs \
  --filters 'Name=tag:kleya:managed,Values=true' \
  --query 'KeyPairs[].[KeyName,KeyPairId,CreateTime]' \
  --output table
```

## Cost shape

Each full e2e run launches one spot `t4g.small` for under 5 minutes:
roughly $0.001–$0.005 per run. Failed cleanup is the only real cost
risk — hence the tagging discipline above.
