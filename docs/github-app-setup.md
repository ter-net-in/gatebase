# GitHub App Setup

Gatebase uses a GitHub App to validate issue access signals, comment one-time tokens, and close approved access issues.

## Create The App

1. Open GitHub organization settings.
2. Go to Developer settings > GitHub Apps > New GitHub App.
3. Set a name such as `gatebase-prod-access`.
4. Set Webhook URL to `https://<broker-host>/webhooks/github`.
5. Generate a webhook secret and save it for `github.webhook_secret`.
6. Create the app, then generate a private key PEM.
7. Install the app on repositories that Gatebase may validate.

## Required Permissions

Set repository permissions:

- Metadata: Read
- Issues: Read and write

Subscribe to events:

- Issues

## Find IDs

Use the app settings page for `app_id`.

Use the app installation URL or API for `installation_id`. Installation URLs include the ID:

```text
https://github.com/organizations/<org>/settings/installations/<installation_id>
```

## Configure Gatebase

```yaml
github:
  app_id: "123456"
  installation_id: 987654
  private_key_file: "/etc/gatebase/github-app.pem"
  webhook_secret: "change-me"
  api_base_url: "https://api.github.com"

targets:
  - name: "prod-pg"
    access:
      github_repo: "org/repo"
      required_signals:
        - type: "github_issue_open"
        - type: "github_issue_labels"
          labels:
            - "approved"
```

## Validation Behavior

Gatebase checks configured issue signals when GitHub sends an `issues` webhook.
When they pass, Gatebase comments a one-time token and closes the issue.

- `github_issue_open`: issue must exist and be open.
- `github_issue_labels`: every configured label must be present on the issue.

The repository configured on a target is used to infer the target from the webhook.
Repos must be unique across targets.

## Webhook Validation

Gatebase validates `X-Hub-Signature-256` with `github.webhook_secret` using HMAC-SHA256. Invalid signatures return `401`.

Webhooks mint short-lived one-time tokens for approved issues.
