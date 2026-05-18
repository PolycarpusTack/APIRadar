import { HelpCircle, Terminal, Key, FileText, FlaskConical, Zap } from 'lucide-react'
import PageHeader from '../components/PageHeader'

// ---------------------------------------------------------------------------
// Primitives
// ---------------------------------------------------------------------------

function Section({ title, icon: Icon, children }: { title: string; icon: typeof HelpCircle; children: React.ReactNode }) {
  return (
    <section className="space-y-4">
      <div className="flex items-center gap-2">
        <Icon className="h-4 w-4 flex-shrink-0" style={{ color: 'var(--cobalt-mid)' }} />
        <h2 className="text-[13px] font-semibold" style={{ color: 'var(--text-1)' }}>{title}</h2>
      </div>
      {children}
    </section>
  )
}

function Card({ children }: { children: React.ReactNode }) {
  return (
    <div className="rounded-lg p-5 space-y-3" style={{ background: 'var(--bg-surface)', border: '1px solid var(--border)' }}>
      {children}
    </div>
  )
}

function Code({ children }: { children: React.ReactNode }) {
  return (
    <code
      className="rounded px-1.5 py-0.5 text-[11.5px]"
      style={{ background: 'var(--bg-raised)', border: '1px solid var(--border)', color: 'var(--teal)', fontFamily: 'var(--font-mono)' }}
    >
      {children}
    </code>
  )
}

function Block({ children }: { children: string }) {
  return (
    <pre
      className="rounded-lg p-4 text-[12px] leading-relaxed overflow-x-auto"
      style={{ background: 'var(--bg-raised)', border: '1px solid var(--border)', fontFamily: 'var(--font-mono)', color: 'var(--text-2)' }}
    >
      {children}
    </pre>
  )
}

function Flag({ name, desc }: { name: string; desc: string }) {
  return (
    <div className="flex gap-3 text-[12.5px]">
      <span className="shrink-0 w-64" style={{ fontFamily: 'var(--font-mono)', color: 'var(--teal)' }}>{name}</span>
      <span style={{ color: 'var(--text-2)' }}>{desc}</span>
    </div>
  )
}

function EnvVar({ name, desc }: { name: string; desc: string }) {
  return (
    <div className="flex gap-3 text-[12.5px] py-1.5" style={{ borderBottom: '1px solid var(--border)' }}>
      <span className="shrink-0 w-56" style={{ fontFamily: 'var(--font-mono)', color: 'var(--amber)' }}>{name}</span>
      <span style={{ color: 'var(--text-2)' }}>{desc}</span>
    </div>
  )
}

function Step({ n, title, children }: { n: number; title: string; children: React.ReactNode }) {
  return (
    <div className="flex gap-4">
      <div
        className="flex h-6 w-6 shrink-0 items-center justify-center rounded-full text-[11px] font-bold mt-0.5"
        style={{ background: 'var(--cobalt)', color: 'var(--text-inverse)' }}
      >
        {n}
      </div>
      <div className="space-y-2 flex-1">
        <p className="text-[13px] font-semibold" style={{ color: 'var(--text-1)' }}>{title}</p>
        {children}
      </div>
    </div>
  )
}

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

export default function HelpPage() {
  return (
    <div className="p-8 max-w-4xl mx-auto space-y-10">
      <PageHeader
        tag="Help"
        title="Help & Reference"
        description="Command reference, environment variables, and feature walkthroughs for APIRadar"
      />

      {/* ------------------------------------------------------------------ */}
      {/* Quick Start                                                           */}
      {/* ------------------------------------------------------------------ */}
      <Section title="Quick Start — 5-step workflow" icon={Zap}>
        <Card>
          <div className="space-y-5">
            <Step n={1} title="Start the API server">
              <Block>{`radar-api --db sqlite:radar.db --bind 127.0.0.1:8080`}</Block>
              <p className="text-[12px]" style={{ color: 'var(--text-3)' }}>
                Or via Docker: <Code>docker compose up</Code> (uses PostgreSQL).
                Set <Code>RADAR_SERVICE_TOKEN</Code> to protect the API.
              </p>
            </Step>

            <Step n={2} title="Check a diff in CI">
              <Block>{`radar check \\
  --base old.yaml --head new.yaml \\
  --api-url http://localhost:8080 \\
  --service-id <uuid> \\
  --post-comment          # posts to GitHub PR`}</Block>
              <p className="text-[12px]" style={{ color: 'var(--text-3)' }}>
                Exits <Code>0</Code> (clean), <Code>1</Code> (breaking changes), or <Code>2</Code> (parse error).
                Spec format is auto-detected from the file extension (<Code>.yaml</Code> = OpenAPI, <Code>.graphql</Code> = GraphQL SDL, <Code>.proto</Code> = protobuf).
              </p>
            </Step>

            <Step n={3} title="Register consumers">
              <Block>{`radar register \\
  --api-url http://localhost:8080 \\
  --service-id <producer-uuid> \\
  --consumer-name checkout-svc \\
  --repo-url https://github.com/org/checkout \\
  --owner-team payments \\
  --contact ops@example.com`}</Block>
              <p className="text-[12px]" style={{ color: 'var(--text-3)' }}>
                Creates the consumer and subscribes it to the producer in one call.
                Once registered, blast-radius reports will include this consumer.
              </p>
            </Step>

            <Step n={4} title="Scan consumer repos for call sites (optional)">
              <Block>{`radar scan \\
  --consumer-id <uuid> \\
  --service-id <uuid> \\
  --source-dir ./checkout-svc \\
  --api-url http://localhost:8080`}</Block>
              <p className="text-[12px]" style={{ color: 'var(--text-3)' }}>
                Uses tree-sitter to find field accesses in TypeScript, Python, Go, Rust, and Java.
                Results are stored as Call Sites and increase blast-radius confidence scores.
              </p>
            </Step>

            <Step n={5} title="Generate release notes or Postman tests">
              <Block>{`# Markdown release notes with per-consumer migration guides
radar explain \\
  --diff-id <uuid> --api-url http://localhost:8080 \\
  --release-notes --migration-guide \\
  --out RELEASE_NOTES.md

# Postman test collection from a Jira ticket
radar generate-tests \\
  --jira PROJ-123 --spec openapi.yaml \\
  --out tests.postman_collection.json`}</Block>
            </Step>
          </div>
        </Card>
      </Section>

      {/* ------------------------------------------------------------------ */}
      {/* CLI Reference                                                         */}
      {/* ------------------------------------------------------------------ */}
      <Section title="CLI Command Reference" icon={Terminal}>
        <div className="space-y-4">

          {/* radar check */}
          <Card>
            <p className="text-[12.5px] font-semibold" style={{ color: 'var(--text-1)' }}>
              <Code>radar check</Code> — Compare two spec versions and report breaking changes
            </p>
            <div className="space-y-1.5 pt-1">
              <Flag name="--base <path>"       desc="Base spec file (older version)" />
              <Flag name="--head <path>"       desc="Head spec file (newer version)" />
              <Flag name="--format <fmt>"      desc="openapi | graphql | protobuf  (auto-detected from extension)" />
              <Flag name="--policy <path>"     desc="Path to .radar.yml policy file (default: .radar.yml in cwd)" />
              <Flag name="--api-url <url>"     desc="radar-api base URL (also RADAR_API_URL env var)" />
              <Flag name="--service-id <uuid>" desc="Producer service UUID — enables diff posting and blast-radius lookup" />
              <Flag name="--token <token>"     desc="Bearer token for the API (also RADAR_SERVICE_TOKEN env var)" />
              <Flag name="--post-comment"      desc="Post or update a GitHub PR comment (needs GITHUB_TOKEN)" />
              <Flag name="--json"              desc="Emit machine-readable JSON to stdout instead of a colour table" />
              <Flag name="--no-color"          desc="Disable ANSI colour output (also honoured via NO_COLOR env var)" />
            </div>
            <p className="text-[11.5px] pt-1" style={{ color: 'var(--text-3)' }}>
              Exit codes: <Code>0</Code> clean · <Code>1</Code> breaking changes (per policy) · <Code>2</Code> parse / config error
            </p>
          </Card>

          {/* radar register */}
          <Card>
            <p className="text-[12.5px] font-semibold" style={{ color: 'var(--text-1)' }}>
              <Code>radar register</Code> — Register a consumer and subscribe it to a producer
            </p>
            <div className="space-y-1.5 pt-1">
              <Flag name="--api-url <url>"        desc="radar-api base URL (also RADAR_API_URL env var)" />
              <Flag name="--service-id <uuid>"    desc="Producer service UUID to subscribe to" />
              <Flag name="--consumer-name <name>" desc="Display name for this consumer" />
              <Flag name="--repo-url <url>"       desc="Repository URL of the consumer service" />
              <Flag name="--owner-team <team>"    desc="Team name responsible for this consumer" />
              <Flag name="--contact <email>"      desc="Contact address for blast-radius notifications" />
              <Flag name="--token <token>"        desc="Bearer token for the API" />
            </div>
          </Card>

          {/* radar scan */}
          <Card>
            <p className="text-[12.5px] font-semibold" style={{ color: 'var(--text-1)' }}>
              <Code>radar scan</Code> — Extract API call sites from consumer source code
            </p>
            <div className="space-y-1.5 pt-1">
              <Flag name="--consumer-id <uuid>" desc="UUID of the consumer to scan" />
              <Flag name="--service-id <uuid>"  desc="UUID of the producer whose fields to track" />
              <Flag name="--source-dir <path>"  desc="Root directory to scan (TypeScript, Python, Go, Rust, Java)" />
              <Flag name="--api-url <url>"      desc="radar-api base URL" />
              <Flag name="--token <token>"      desc="Bearer token for the API" />
            </div>
            <p className="text-[11.5px] pt-1" style={{ color: 'var(--text-3)' }}>
              Uses tree-sitter grammars. Results are posted to <Code>POST /v1/call-sites</Code> in batches of 500.
            </p>
          </Card>

          {/* radar explain */}
          <Card>
            <p className="text-[12.5px] font-semibold" style={{ color: 'var(--text-1)' }}>
              <Code>radar explain</Code> — Generate release notes and migration guides for a diff
            </p>
            <div className="space-y-1.5 pt-1">
              <Flag name="--diff-id <uuid>"       desc="UUID of the diff to explain (shown in the Diffs page)" />
              <Flag name="--api-url <url>"         desc="radar-api base URL" />
              <Flag name="--release-notes"         desc="Generate Markdown release notes to stdout or --out file" />
              <Flag name="--migration-guide"       desc="Add per-consumer migration guides via Claude (needs ANTHROPIC_API_KEY)" />
              <Flag name="--post-github-release"   desc="Create a GitHub Release with the notes (needs GITHUB_TOKEN)" />
              <Flag name="--out <path>"            desc="Write output to a file instead of stdout" />
            </div>
          </Card>

          {/* radar completions */}
          <Card>
            <p className="text-[12.5px] font-semibold" style={{ color: 'var(--text-1)' }}>
              <Code>radar completions &lt;shell&gt;</Code> — Print shell completion script to stdout
            </p>
            <p className="text-[12.5px] pt-1" style={{ color: 'var(--text-3)' }}>
              Supported shells: <Code>bash</Code>, <Code>zsh</Code>, <Code>fish</Code>, <Code>powershell</Code>, <Code>elvish</Code>.
            </p>
            <Block>{`# bash
radar completions bash >> ~/.bash_completion

# zsh (oh-my-zsh)
radar completions zsh > ~/.oh-my-zsh/completions/_radar

# fish
radar completions fish > ~/.config/fish/completions/radar.fish`}</Block>
          </Card>

          {/* radar generate-tests */}
          <Card>
            <p className="text-[12.5px] font-semibold" style={{ color: 'var(--text-1)' }}>
              <Code>radar generate-tests</Code> — Generate a Postman Collection from a Jira ticket + spec
            </p>
            <div className="space-y-1.5 pt-1">
              <Flag name="--jira <key>"              desc="Jira ticket key e.g. PROJ-123 (uses JIRA_BASE_URL / JIRA_EMAIL / JIRA_TOKEN)" />
              <Flag name={'--jira-text "..."'}         desc="Paste ticket text directly (fallback when Jira credentials are absent)" />
              <Flag name="--spec <path>"             desc="Path to the OpenAPI YAML/JSON spec file (required)" />
              <Flag name="--base-url <url>"          desc="API base URL inserted into every generated request (default: http://localhost:8080)" />
              <Flag name="--out <path>"              desc="Write the Postman Collection JSON to a file (default: stdout)" />
              <Flag name="--postman-workspace <id>"  desc="Push the collection to this Postman workspace (needs POSTMAN_API_KEY)" />
            </div>
            <p className="text-[11.5px] pt-1" style={{ color: 'var(--text-3)' }}>
              Requires <Code>ANTHROPIC_API_KEY</Code>. Claude generates happy-path and negative test cases;
              the CLI assembles them into a spec-compliant Postman Collection v2.1.
              The result can also be generated from the <strong>Generate Tests</strong> page in the dashboard.
            </p>
          </Card>
        </div>
      </Section>

      {/* ------------------------------------------------------------------ */}
      {/* Policy file                                                           */}
      {/* ------------------------------------------------------------------ */}
      <Section title="Policy File (.radar.yml)" icon={FileText}>
        <Card>
          <p className="text-[12.5px]" style={{ color: 'var(--text-2)' }}>
            Place a <Code>.radar.yml</Code> file in your repo root to control CI behaviour without changing the command invocation.
          </p>
          <Block>{`policy:
  # never | any_break | active_consumers (default)
  block_on: active_consumers

  # Allow a PR to pass even with breaking changes if it carries
  # the specified GitHub label. Remove to disable label overrides.
  allow_override_with: "label:radar-ack"`}</Block>
          <div className="space-y-1.5 text-[12.5px]">
            <Flag name="block_on: never"            desc="Never block CI — only warnings in the PR comment" />
            <Flag name="block_on: any_break"        desc="Block on any breaking change (exit 1)" />
            <Flag name="block_on: active_consumers" desc="Block only when at least one subscribed consumer is active (default)" />
            <Flag name="allow_override_with: ..."   desc="label:radar-ack — adds a label-based escape hatch for approved exceptions" />
          </div>
        </Card>
      </Section>

      {/* ------------------------------------------------------------------ */}
      {/* Environment variables                                                 */}
      {/* ------------------------------------------------------------------ */}
      <Section title="Environment Variables" icon={Key}>
        <Card>
          <div className="divide-y" style={{ borderColor: 'var(--border)' }}>
            <div className="pb-2">
              <p className="text-[10.5px] font-semibold uppercase tracking-wider pb-2" style={{ color: 'var(--text-dim)' }}>
                Core API
              </p>
              <EnvVar name="RADAR_API_URL"        desc="Base URL of the radar-api server used by the CLI (e.g. http://localhost:8080)" />
              <EnvVar name="RADAR_SERVICE_TOKEN"  desc="Static bearer token that the CLI sends and the server validates" />
              <EnvVar name="RADAR_JWT_SECRET"     desc="HS256 JWT secret on the server. When set, overrides static token auth; CLI must send a signed JWT" />
            </div>
            <div className="py-2">
              <p className="text-[10.5px] font-semibold uppercase tracking-wider pb-2 pt-3" style={{ color: 'var(--text-dim)' }}>
                AI & Integrations
              </p>
              <EnvVar name="ANTHROPIC_API_KEY"    desc="Required for radar explain --migration-guide and radar generate-tests" />
              <EnvVar name="JIRA_BASE_URL"        desc="Your Jira Cloud base URL (e.g. https://mycompany.atlassian.net)" />
              <EnvVar name="JIRA_EMAIL"           desc="Atlassian account email for Jira Basic auth" />
              <EnvVar name="JIRA_TOKEN"           desc="Atlassian API token for Jira Basic auth" />
              <EnvVar name="POSTMAN_API_KEY"      desc="Postman API key for --postman-workspace push" />
            </div>
            <div className="py-2">
              <p className="text-[10.5px] font-semibold uppercase tracking-wider pb-2 pt-3" style={{ color: 'var(--text-dim)' }}>
                GitHub (CI)
              </p>
              <EnvVar name="GITHUB_TOKEN"         desc="Required for --post-comment (PR comments) and --post-github-release" />
              <EnvVar name="GITHUB_REPOSITORY"    desc="Set automatically by GitHub Actions (owner/repo format)" />
              <EnvVar name="GITHUB_REF"           desc="Set automatically by GitHub Actions — used to extract the PR number" />
            </div>
            <div className="py-2">
              <p className="text-[10.5px] font-semibold uppercase tracking-wider pb-2 pt-3" style={{ color: 'var(--text-dim)' }}>
                Server tuning
              </p>
              <EnvVar name="RATE_LIMIT_PER_MINUTE"  desc="Max requests per IP per minute (default 300, 0 = unlimited)" />
              <EnvVar name="MAX_BODY_SIZE_MB"        desc="Maximum request body size in megabytes (default 4 MB)" />
              <EnvVar name="CORS_ALLOWED_ORIGINS"    desc="Comma-separated list of allowed CORS origins. Omit for permissive (dev). Example: https://app.example.com" />
              <EnvVar name="DATABASE_URL"            desc="sqlx connection string: sqlite:radar.db or postgres://user:pass@host/db" />
              <EnvVar name="BIND_ADDR"               desc="Socket address to listen on (default 0.0.0.0:8080; use 127.0.0.1:8080 in desktop sidecar mode)" />
            </div>
          </div>
        </Card>
      </Section>

      {/* ------------------------------------------------------------------ */}
      {/* Generate Tests walkthrough                                            */}
      {/* ------------------------------------------------------------------ */}
      <Section title="Generate Postman Tests — feature walkthrough" icon={FlaskConical}>
        <Card>
          <p className="text-[12.5px]" style={{ color: 'var(--text-2)' }}>
            APIRadar uses Claude to read a Jira ticket and your OpenAPI spec and produce a
            Postman Collection v2.1 with happy-path and negative test cases already wired up.
          </p>

          <div className="space-y-3 pt-1">
            <p className="text-[12px] font-semibold" style={{ color: 'var(--text-1)' }}>Option A — via the CLI</p>
            <Block>{`# With Jira credentials (fetches ticket automatically)
export JIRA_BASE_URL=https://mycompany.atlassian.net
export JIRA_EMAIL=you@example.com
export JIRA_TOKEN=<atlassian-api-token>
export ANTHROPIC_API_KEY=<key>

radar generate-tests \\
  --jira PROJ-123 \\
  --spec docs/openapi.yaml \\
  --base-url https://api.example.com \\
  --out PROJ-123.postman_collection.json

# Without Jira credentials — paste the ticket text
radar generate-tests \\
  --jira-text "As a user I want to create an account…" \\
  --spec docs/openapi.yaml \\
  --out tests.json

# Push directly into a Postman workspace
export POSTMAN_API_KEY=<key>
radar generate-tests --jira PROJ-123 --spec openapi.yaml \\
  --postman-workspace <workspace-uuid>`}</Block>

            <p className="text-[12px] font-semibold" style={{ color: 'var(--text-1)' }}>Option B — via the dashboard</p>
            <p className="text-[12.5px]" style={{ color: 'var(--text-2)' }}>
              Go to <strong>Testing → Generate Tests</strong> in the sidebar.
              Paste the Jira key (or the ticket text), paste the spec YAML, set the base URL, and click Generate.
              The result card shows the happy-path / negative breakdown with a Download button.
              Previously generated suites are listed below the form.
            </p>

            <p className="text-[12px] font-semibold" style={{ color: 'var(--text-1)' }}>What Claude generates</p>
            <p className="text-[12.5px]" style={{ color: 'var(--text-2)' }}>
              4–6 happy-path tests covering the ticket's acceptance criteria, and 4–8 negative tests
              (missing required fields → 422, invalid types → 400, unauthorized → 401, not-found → 404).
              Each item includes a <Code>pm.test()</Code> assertion block ready to run in Postman or Newman.
              <Code>{'{{baseUrl}}'}</Code> and <Code>{'{{authToken}}'}</Code> are collection variables you set once per environment.
            </p>
          </div>
        </Card>
      </Section>
    </div>
  )
}
