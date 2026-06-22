# GitHub App Setup

Gatebase uses a GitHub App to validate pull request access signals before issuing database sessions.

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

- Pull requests: Read
- Checks: Read
- Commit statuses: Read
- Contents: Read
- Metadata: Read

Subscribe to events:

- Pull request
- Pull request review
- Check run
- Check suite
- Status

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

access:
  allowed_repositories:
    - "org/repo"
  required_signals:
    - type: "github_pull_request_open"
    - type: "github_pull_request_approved"
    - type: "github_checks_passed"
      checks:
        - "ci"
    - type: "github_labels"
      labels:
        - "db-access-approved"
    - type: "github_codeowners_reviewed"
```

## Validation Behavior

Gatebase checks configured signals synchronously when `POST /api/sessions` is called.

- `github_pull_request_open`: PR must exist and be open.
- `github_pull_request_approved`: latest review state from at least one reviewer must be `APPROVED`.
- `github_checks_passed`: every configured check run or commit status must be successful.
- `github_labels`: every configured label must be present on the PR.
- `github_codeowners_reviewed`: best-effort check that no requested reviewers or teams remain and a current approval exists.

Exact CODEOWNERS ownership parsing and team membership expansion are not implemented yet.

## Webhook Validation

Gatebase validates `X-Hub-Signature-256` with `github.webhook_secret` using HMAC-SHA256. Invalid signatures return `401`.

Webhooks currently provide authenticated event intake. Session creation does not depend on cached webhook state yet.
