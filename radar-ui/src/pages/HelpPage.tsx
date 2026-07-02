import { useState } from 'react'
import {
  HelpCircle, Terminal, Key, FileText, FlaskConical, Zap,
  BookOpen, Telescope, GitCompare, Users, LayoutDashboard,
  Server, AlertTriangle, Lightbulb, MessageSquare, CheckCircle2,
  ArrowRight, Sparkles, Bell, Clock, Shield, GitBranch, Database, BarChart2,
} from 'lucide-react'
import PageHeader from '../components/PageHeader'

// ---------------------------------------------------------------------------
// Shared primitives
// ---------------------------------------------------------------------------

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

function Card({ children, className = '' }: { children: React.ReactNode; className?: string }) {
  return (
    <div
      className={`rounded-lg p-5 space-y-3 ${className}`}
      style={{ background: 'var(--bg-surface)', border: '1px solid var(--border)' }}
    >
      {children}
    </div>
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

function RefSection({ title, icon: Icon, children }: { title: string; icon: typeof HelpCircle; children: React.ReactNode }) {
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
// Beginner guide primitives
// ---------------------------------------------------------------------------

type CalloutVariant = 'tip' | 'warning' | 'analogy' | 'question' | 'celebrate'

const CALLOUT_CONFIG: Record<CalloutVariant, { icon: typeof Lightbulb; label: string; border: string; bg: string; label_color: string }> = {
  tip: {
    icon: Lightbulb,
    label: 'Pro Tip',
    border: 'var(--cobalt)',
    bg: 'rgba(56,5,227,0.07)',
    label_color: 'var(--cobalt-mid)',
  },
  warning: {
    icon: AlertTriangle,
    label: 'Watch Out',
    border: 'var(--amber)',
    bg: 'rgba(251,191,36,0.07)',
    label_color: 'var(--amber)',
  },
  analogy: {
    icon: MessageSquare,
    label: 'Think of it like…',
    border: 'var(--teal)',
    bg: 'rgba(20,184,166,0.07)',
    label_color: 'var(--teal)',
  },
  question: {
    icon: HelpCircle,
    label: 'You might be wondering…',
    border: 'var(--text-3)',
    bg: 'var(--bg-hover)',
    label_color: 'var(--text-2)',
  },
  celebrate: {
    icon: CheckCircle2,
    label: 'You got it!',
    border: 'var(--green, #34d399)',
    bg: 'rgba(52,211,153,0.07)',
    label_color: 'var(--green, #34d399)',
  },
}

function Callout({ variant, children }: { variant: CalloutVariant; children: React.ReactNode }) {
  const cfg = CALLOUT_CONFIG[variant]
  const Icon = cfg.icon
  return (
    <div
      className="rounded-lg p-4 flex gap-3"
      style={{ background: cfg.bg, borderLeft: `3px solid ${cfg.border}` }}
    >
      <Icon className="h-4 w-4 shrink-0 mt-0.5" style={{ color: cfg.border }} />
      <div className="space-y-1 flex-1">
        <p className="text-[11px] font-bold uppercase tracking-wider" style={{ color: cfg.label_color }}>
          {cfg.label}
        </p>
        <div className="text-[12.5px] leading-relaxed" style={{ color: 'var(--text-2)' }}>
          {children}
        </div>
      </div>
    </div>
  )
}

function TourStop({
  icon: Icon,
  page,
  badge,
  tagline,
  children,
}: {
  icon: typeof LayoutDashboard
  page: string
  badge: string
  tagline: string
  children: React.ReactNode
}) {
  return (
    <div
      className="rounded-lg p-5 space-y-3"
      style={{ background: 'var(--bg-surface)', border: '1px solid var(--border)' }}
    >
      <div className="flex items-center gap-3">
        <div
          className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg"
          style={{ background: 'var(--bg-active)' }}
        >
          <Icon className="h-4 w-4" style={{ color: 'var(--cobalt-mid)' }} />
        </div>
        <div>
          <div className="flex items-center gap-2">
            <span className="text-[13px] font-semibold" style={{ color: 'var(--text-1)' }}>{page}</span>
            <span
              className="rounded px-1.5 py-0.5 text-[9.5px] font-semibold uppercase tracking-wider"
              style={{ background: 'var(--bg-raised)', color: 'var(--text-dim)' }}
            >
              {badge}
            </span>
          </div>
          <p className="text-[11.5px] italic" style={{ color: 'var(--text-3)' }}>{tagline}</p>
        </div>
      </div>
      <div className="text-[12.5px] leading-relaxed space-y-2" style={{ color: 'var(--text-2)' }}>
        {children}
      </div>
    </div>
  )
}

function GuideStep({ n, emoji, title, children }: { n: number; emoji: string; title: string; children: React.ReactNode }) {
  return (
    <div className="flex gap-4">
      <div className="flex flex-col items-center gap-1 shrink-0">
        <div
          className="flex h-7 w-7 items-center justify-center rounded-full text-[12px] font-bold"
          style={{ background: 'var(--cobalt)', color: 'var(--text-inverse)' }}
        >
          {n}
        </div>
        <div className="w-px flex-1" style={{ background: 'var(--border)', minHeight: '20px' }} />
      </div>
      <div className="pb-6 space-y-2 flex-1">
        <p className="text-[13.5px] font-semibold" style={{ color: 'var(--text-1)' }}>
          {emoji} {title}
        </p>
        <div className="text-[12.5px] leading-relaxed space-y-2" style={{ color: 'var(--text-2)' }}>
          {children}
        </div>
      </div>
    </div>
  )
}

function FAQ({ q, children }: { q: string; children: React.ReactNode }) {
  const [open, setOpen] = useState(false)
  return (
    <div style={{ borderBottom: '1px solid var(--border)' }}>
      <button
        className="w-full flex items-center justify-between py-3 text-left gap-4"
        onClick={() => setOpen((o) => !o)}
      >
        <span className="text-[12.5px] font-medium" style={{ color: 'var(--text-1)' }}>{q}</span>
        <ArrowRight
          className="h-3.5 w-3.5 shrink-0 transition-transform"
          style={{ color: 'var(--text-3)', transform: open ? 'rotate(90deg)' : '' }}
        />
      </button>
      {open && (
        <div className="pb-3 text-[12.5px] leading-relaxed" style={{ color: 'var(--text-2)' }}>
          {children}
        </div>
      )}
    </div>
  )
}

// ---------------------------------------------------------------------------
// Tab switcher
// ---------------------------------------------------------------------------

type Tab = 'guide' | 'reference'

function TabBar({ active, onChange }: { active: Tab; onChange: (t: Tab) => void }) {
  return (
    <div
      className="flex gap-1 p-1 rounded-lg"
      style={{ background: 'var(--bg-surface)', border: '1px solid var(--border)', display: 'inline-flex' }}
    >
      {([['guide', BookOpen, 'Beginner\'s Guide'], ['reference', Terminal, 'CLI Reference']] as const).map(
        ([id, Icon, label]) => (
          <button
            key={id}
            onClick={() => onChange(id)}
            className="flex items-center gap-2 rounded-md px-4 py-2 text-[12.5px] font-medium transition-all"
            style={
              active === id
                ? { background: 'var(--cobalt)', color: 'var(--text-inverse)' }
                : { color: 'var(--text-2)' }
            }
          >
            <Icon className="h-3.5 w-3.5" />
            {label}
          </button>
        ),
      )}
    </div>
  )
}

// ---------------------------------------------------------------------------
// Beginner's Guide tab
// ---------------------------------------------------------------------------

function BeginnersGuide() {
  return (
    <div className="space-y-10">

      {/* ------------------------------------------------------------------ */}
      {/* Hero                                                                  */}
      {/* ------------------------------------------------------------------ */}
      <div
        className="rounded-xl p-8 space-y-3"
        style={{
          background: 'linear-gradient(135deg, rgba(56,5,227,0.12) 0%, rgba(56,5,227,0.04) 100%)',
          border: '1px solid rgba(56,5,227,0.2)',
        }}
      >
        <div className="flex items-center gap-2 mb-1">
          <Sparkles className="h-5 w-5" style={{ color: 'var(--cobalt-mid)' }} />
          <span className="text-[11px] font-bold uppercase tracking-widest" style={{ color: 'var(--cobalt-mid)' }}>
            New here? Start here.
          </span>
        </div>
        <h2 className="text-[22px] font-bold leading-snug" style={{ color: 'var(--text-1)', fontFamily: 'var(--font-head)' }}>
          API Contract Radar Monitor<br />
          <span style={{ color: 'var(--cobalt-mid)' }}>explained like you're five.</span>
        </h2>
        <p className="text-[13.5px] leading-relaxed max-w-2xl" style={{ color: 'var(--text-2)' }}>
          No engineering degree required. This guide walks you through every page of the dashboard,
          tells you what each thing means in plain English, and gets you to your first "aha!" moment
          in under 10 minutes.
        </p>
      </div>

      {/* ------------------------------------------------------------------ */}
      {/* What is this thing?                                                   */}
      {/* ------------------------------------------------------------------ */}
      <section className="space-y-4">
        <h3 className="text-[15px] font-bold" style={{ color: 'var(--text-1)' }}>
          Chapter 1 — What even is an "API contract"?
        </h3>

        <Card>
          <p className="text-[13px] leading-relaxed" style={{ color: 'var(--text-2)' }}>
            Modern apps are made of dozens (sometimes hundreds) of small programs — called
            <strong style={{ color: 'var(--text-1)' }}> services</strong> — that talk to each other
            over the internet. The way they talk is called an <strong style={{ color: 'var(--text-1)' }}>API</strong>.
            An <strong style={{ color: 'var(--text-1)' }}>API contract</strong> is the agreed-on "language"
            of those conversations: which fields exist, what types they are, and what responses to expect.
          </p>
          <Callout variant="analogy">
            <p>
              Imagine two apps communicating like two colleagues filling out the same paper form every day.
              The form has boxes labelled <em>"first_name"</em>, <em>"email"</em>, <em>"account_id"</em>.
              Both sides know exactly what to write and where to look.
            </p>
            <p className="mt-2">
              Now imagine one colleague quietly renames <em>"account_id"</em> to <em>"id"</em> without telling anyone.
              The other colleague keeps looking for <em>"account_id"</em> and finds nothing — the app breaks.
            </p>
            <p className="mt-2 font-semibold" style={{ color: 'var(--teal)' }}>
              That renamed field is a <em>breaking change</em>. Radar is the smoke detector that catches it before it causes a fire.
            </p>
          </Callout>
        </Card>

        <Card>
          <p className="text-[13px] font-semibold" style={{ color: 'var(--text-1)' }}>
            The three main actors in Radar Monitor
          </p>
          <div className="space-y-3 pt-1">
            {[
              ['Producer', 'The service that owns the API — it defines the form.', 'var(--cobalt-mid)'],
              ['Consumer', 'The service that uses the API — it fills in the form.', 'var(--teal)'],
              ['Blast Radius', 'All the Consumers that would break if the Producer made a change. The more consumers, the bigger the blast radius.', 'var(--amber)'],
            ].map(([term, def, color]) => (
              <div key={term} className="flex gap-3">
                <span
                  className="shrink-0 rounded-md px-2 py-0.5 text-[11px] font-bold"
                  style={{ background: `${color}20`, color, border: `1px solid ${color}40` }}
                >
                  {term}
                </span>
                <p className="text-[12.5px]" style={{ color: 'var(--text-2)' }}>{def}</p>
              </div>
            ))}
          </div>
        </Card>

        <Callout variant="question">
          <p><strong>Do I need to be a developer to use this?</strong></p>
          <p className="mt-1">
            Not for the dashboard! Product managers, QA engineers, and tech leads use the UI every day
            to review what changed, who is affected, and whether a release is safe. The command-line tool
            (CLI) is for developers who run checks inside automated pipelines.
          </p>
        </Callout>
      </section>

      {/* ------------------------------------------------------------------ */}
      {/* Tour of the dashboard                                                 */}
      {/* ------------------------------------------------------------------ */}
      <section className="space-y-4">
        <h3 className="text-[15px] font-bold" style={{ color: 'var(--text-1)' }}>
          Chapter 2 — A quick tour of the dashboard
        </h3>
        <p className="text-[12.5px]" style={{ color: 'var(--text-3)' }}>
          Look at the sidebar on the left. Here's what every page does in one sentence.
        </p>

        <div className="space-y-3">
          <TourStop icon={LayoutDashboard} page="Overview" badge="Monitor" tagline="Your mission control">
            <p>
              The first screen you see. It shows live stats: how many services are registered,
              how many consumers are watching them, how many diffs (comparisons) have been run,
              and how many breaking changes were caught. Think of it as the headline news for your API estate.
            </p>
            <Callout variant="tip">
              If the breaking-changes number is going up over time, your teams are shipping fast. That's good!
              Just make sure the blast radius on each one is small.
            </Callout>
          </TourStop>

          <TourStop icon={GitCompare} page="Diffs" badge="Monitor" tagline="Every comparison ever run">
            <p>
              A <strong style={{ color: 'var(--text-1)' }}>Diff</strong> is what Radar creates when
              it compares an old version of an API spec to a new one. This page lists every diff ever run,
              colour-coded by severity.
            </p>
            <p>
              Click any row to open the detail view: you'll see the exact list of changes (with field paths
              like <em>"user.address.postalCode"</em>), which consumers are in the blast radius, and a
              <strong style={{ color: 'var(--text-1)' }}> Generate Release Notes</strong> button that
              instantly produces a structured Markdown changelog from the stored changes.
            </p>
            <p>
              The <strong style={{ color: 'var(--text-1)' }}>Compare Specs</strong> button (top-right toolbar)
              opens a side-by-side editor where you can paste or upload two spec files and run a diff
              without needing the CLI at all.
            </p>
            <Callout variant="analogy">
              A Diff is like a "track changes" view in Word — except instead of paragraphs, you're
              tracking API fields. Red means something was removed or renamed. Green means something was added.
            </Callout>
          </TourStop>

          <TourStop icon={Server} page="Services" badge="Registry" tagline="The APIs being monitored">
            <p>
              A <strong style={{ color: 'var(--text-1)' }}>Service</strong> (also called a Producer)
              is any app whose API you want to monitor. You register it once, give it a name and an owner,
              and Radar starts tracking its spec history.
            </p>
            <p>
              Each service card shows the last time a diff was run, the number of consumers subscribed,
              and whether there are active breaking changes.
            </p>
          </TourStop>

          <TourStop icon={Users} page="Consumers" badge="Registry" tagline="Who depends on what">
            <p>
              A <strong style={{ color: 'var(--text-1)' }}>Consumer</strong> is any app that calls a
              Producer's API. Registering consumers is what turns a diff from "something changed" into
              "these three teams will be broken on Monday."
            </p>
            <p>
              You can register a consumer directly from this page — click
              <strong style={{ color: 'var(--text-1)' }}> Register Consumer</strong>, fill in the name,
              team, and contact email, and subscribe to one or more services. No CLI needed.
              The consumer detail page then shows which fields each consumer has been seen accessing
              and the last time they did.
            </p>
            <Callout variant="tip">
              Even one registered consumer makes the tool dramatically more useful. Start with your most
              critical downstream service.
            </Callout>
          </TourStop>

          <TourStop icon={FileText} page="Release Notes" badge="Docs & Demo" tagline="Auto-generated changelogs">
            <p>
              After a diff is run, you can generate human-readable release notes in two ways:
              click <strong style={{ color: 'var(--text-1)' }}>Generate Release Notes</strong> on
              the Diff detail page for an instant Markdown summary, or use the CLI with
              <code className="mx-1 text-[11.5px]" style={{ color: 'var(--teal)', fontFamily: 'var(--font-mono)' }}>radar explain</code>
              to add AI-powered per-consumer migration guides.
            </p>
            <p>
              This page lists all stored release notes. Click a note to expand it and review the changes.
              Use the status workflow (draft → reviewed → published) to track sign-off before communicating
              changes to consumers.
            </p>
          </TourStop>

          <TourStop icon={Telescope} page="API Playground" badge="Docs & Demo" tagline="Try your API live — no Postman required">
            <p>
              The Playground embeds a fully interactive API explorer (powered by Scalar).
              Load any OpenAPI spec by URL, or pick one directly from Radar's database using the
              <strong style={{ color: 'var(--text-1)' }}> Stored Specs</strong> bar — no copy-pasting needed.
              Call your API endpoints live from the browser.
            </p>
            <p>
              The <strong style={{ color: 'var(--text-1)' }}>Environments</strong> panel is where the
              pre-sales magic happens. Create a named environment with a base URL (your demo tenant)
              and a bearer token (your demo API key). These are saved in the <strong style={{ color: 'var(--text-1)' }}>shared database</strong> —
              not just in your browser — so every teammate who opens Radar sees the same environments
              without any setup on their end.
            </p>
            <p>
              Look for the small indicator in the Environments bar: a teal <strong style={{ color: 'var(--teal)' }}>cloud icon</strong> means
              environments are shared with your team. An amber <strong style={{ color: 'var(--amber)' }}>hard-drive icon</strong> means the
              server is unreachable and Radar has fallen back to browser-only storage.
            </p>
            <Callout variant="analogy">
              Think of it as a pre-configured test-drive lane your whole pre-sales team can use.
              One person sets up the "Base Sandbox" environment once; everyone else just clicks it and goes.
              No Postman workspace, no licence fee, no "can you share that collection with me?"
            </Callout>
            <Callout variant="tip">
              For demos, create one environment per prospect or per environment tier (e.g. "Base Demo",
              "Acme Corp Sandbox", "Staging"). Give each a short description so teammates know which to pick.
            </Callout>
          </TourStop>

          <TourStop icon={FlaskConical} page="Generate Tests" badge="Testing" tagline="Turn a Jira ticket into Postman tests instantly">
            <p>
              Paste a Jira ticket key (or paste the ticket text directly), add your OpenAPI spec, and Radar
              asks an AI to generate a complete Postman test collection — happy-path tests <em>and</em> negative
              tests — in one click.
            </p>
            <p>
              Download the result as a Postman Collection JSON file or an api-testing YAML file and hand it
              to your QA team. The history of every generated suite is saved below the form.
            </p>
            <Callout variant="tip">
              You don't need Jira credentials. The "Switch to paste text" option lets you copy-paste
              the ticket description directly into the box.
            </Callout>
          </TourStop>

          <TourStop icon={Zap} page="CSV Runner" badge="Testing" tagline="Bulk API calls from a spreadsheet — no code needed">
            <p>
              Upload a CSV file where each row is one API call. Define a request template once —
              URL, method, headers, body — using <Code>{'{{column_name}}'}</Code> placeholders, and
              Radar substitutes each row's values and fires the requests in sequence.
            </p>
            <p>
              The progress bar and live counter update as rows complete. When the run finishes,
              you can download the full results as CSV (row number, HTTP status, duration, any error).
            </p>
            <p>
              Before clicking <strong style={{ color: 'var(--text-1)' }}>Run</strong>, check
              <strong style={{ color: 'var(--text-1)' }}> Capture response body</strong> to store the
              first 10 KB of each response alongside the result — expand any row to read the raw
              response inline, without re-running the call.
            </p>
            <Callout variant="tip">
              Transient server errors (HTTP 5xx) are automatically retried up to 3 times with a short
              backoff — so a momentary blip won't fail an entire batch. Client errors (4xx) are not
              retried, as the server already gave a definitive answer.
            </Callout>
            <Callout variant="analogy">
              Think of it like a mail-merge, but for API calls. You prepare the template once; your
              spreadsheet supplies the variables. Radar sends the letters.
            </Callout>
          </TourStop>

          <TourStop icon={Bell} page="Webhooks" badge="Integrations" tagline="Push change events to your own systems">
            <p>
              Register a webhook URL and Radar will POST a JSON payload to it every time a diff is
              created or a breaking change is detected. Use this to trigger Slack messages, open Jira
              tickets, or update a status page — without polling.
            </p>
            <p>
              Each webhook can be tested with a single click (the <strong style={{ color: 'var(--text-1)' }}>Send test</strong> button),
              and the delivery history panel shows the HTTP status and response body of every attempt,
              so you can debug a misconfigured receiver without guessing.
            </p>
            <Callout variant="tip">
              Webhooks respect the <Code>RADAR_ALLOWED_HOSTS</Code> server allowlist — your ops team
              can restrict which external URLs the server is allowed to call.
            </Callout>
          </TourStop>

          <TourStop icon={Clock} page="Scheduled Scans" badge="Integrations" tagline="Automated spec polling on a cron schedule">
            <p>
              Point Radar at any publicly reachable OpenAPI spec URL and give it a cron expression.
              Radar will fetch the spec on schedule, diff it against the last known version, and
              record any changes — no CI pipeline required.
            </p>
            <p>
              The scan run history shows every execution: when it ran, what changed (or "no changes"),
              and any fetch errors. Pair this with a webhook to get notified the moment a third-party
              API you depend on silently changes its contract.
            </p>
            <Callout variant="analogy">
              Think of it as a cron job that watches an API spec instead of a file. If the API
              provider updates their docs, you find out before your consumers find out the hard way.
            </Callout>
          </TourStop>

          <TourStop icon={Shield} page="Audit Trail" badge="Governance" tagline="A tamper-evident log of every action">
            <p>
              Every write operation in Radar — registering a service, running a diff, updating settings,
              triggering a webhook — is recorded in the audit trail with a timestamp, the actor's
              identity, and a metadata snapshot of what changed.
            </p>
            <p>
              Use this page to answer "who changed the default policy?" or "when was consumer X
              registered?" The trail is append-only and scoped to your organisation.
            </p>
            <Callout variant="tip">
              Secrets are automatically redacted from audit metadata — tokens, passwords, and API keys
              are replaced with <Code>[REDACTED]</Code> before storage, so the trail is safe to share
              with auditors.
            </Callout>
          </TourStop>

          <TourStop icon={GitBranch} page="Evolution Rules" badge="Governance" tagline="Allow-list expected changes so they don't block CI">
            <p>
              Sometimes a breaking change is intentional and agreed upon — for example, renaming a
              legacy field across a planned migration window. An <strong style={{ color: 'var(--text-1)' }}>Evolution Rule</strong> tells
              Radar "this specific change is expected; don't fail the CI gate for it."
            </p>
            <p>
              Create a rule on this page, specifying the service, the change kind (e.g. field removed,
              type changed), and an expiry date. The rule is automatically disabled after expiry so
              it can't become a permanent bypass.
            </p>
          </TourStop>

          <TourStop icon={Database} page="Catalog Sources" badge="Governance" tagline="Import your whole API estate in one sync">
            <p>
              Instead of registering services one by one, a <strong style={{ color: 'var(--text-1)' }}>Catalog Source</strong> connects
              Radar to an existing service registry (e.g. Backstage, Apicurio, or a custom JSON/YAML
              endpoint) and imports all registered APIs in bulk.
            </p>
            <p>
              After the initial sync, Radar keeps track of the catalog version so subsequent syncs
              only process what changed. Sync manually from this page, or configure a scheduled scan
              to run it automatically.
            </p>
          </TourStop>

          <TourStop icon={BarChart2} page="Evidence Coverage" badge="Analysis" tagline="See which consumers have real usage data">
            <p>
              Blast-radius accuracy depends on evidence: usage events, static call-site scans, or
              collection file imports. The <strong style={{ color: 'var(--text-1)' }}>Evidence Coverage</strong> page shows,
              per service and per consumer, what fraction of API operations have at least one piece
              of evidence attached.
            </p>
            <p>
              A consumer with low coverage appears in blast-radius reports only for fields it has
              been <em>explicitly seen</em> accessing — meaning you might be underestimating impact.
              Use this page to identify gaps and prioritise which consumers to instrument next.
            </p>
            <Callout variant="tip">
              The sampling configuration panel (accessible from this page) lets you adjust the
              lookback window and minimum-confidence threshold per service.
            </Callout>
          </TourStop>
        </div>
      </section>

      {/* ------------------------------------------------------------------ */}
      {/* Your first workflow                                                    */}
      {/* ------------------------------------------------------------------ */}
      <section className="space-y-4">
        <h3 className="text-[15px] font-bold" style={{ color: 'var(--text-1)' }}>
          Chapter 3 — Your first 10 minutes: a walkthrough
        </h3>
        <p className="text-[12.5px]" style={{ color: 'var(--text-3)' }}>
          Follow these steps in order and you'll have a fully working drift check by the end.
        </p>

        <Card>
          <div className="space-y-0">
            <GuideStep n={1} emoji="🚀" title="Start the server">
              <p>
                Radar Monitor has two parts: this web dashboard, and a small background server (called
                <strong style={{ color: 'var(--text-1)' }}> radar-api</strong>) that stores all the data.
                Your IT person or developer will set this up for you. If you're doing it yourself, run:
              </p>
              <Block>{`radar-api --db sqlite:radar.db --bind 127.0.0.1:8081`}</Block>
              <p>
                Once it's running, the Overview page will load with a green status indicator.
              </p>
              <Callout variant="tip">
                In desktop mode (the installer version), the server starts automatically when you open the app.
                You don't need to do anything.
              </Callout>
            </GuideStep>

            <GuideStep n={2} emoji="📋" title="Register your first Service">
              <p>
                Go to <strong style={{ color: 'var(--text-1)' }}>Services</strong> in the sidebar and click
                "Register Service". Give it the name of the API you want to monitor (for example: "Payments API")
                and your team name. You'll get back a <em>Service ID</em> — copy it, you'll need it in a moment.
              </p>
              <p>
                After saving, Radar shows a blue nudge bar:
                <em style={{ color: 'var(--cobalt-mid)' }}> "Service registered. Ready to compare specs?"</em>
                — click it to go straight to the Compare Specs panel with your new service pre-selected.
              </p>
              <Callout variant="question">
                <p><strong>What is a Service ID?</strong></p>
                <p className="mt-1">It's a unique code that identifies your API in Radar's database. It looks like
                  <code className="mx-1 text-[11px]" style={{ color: 'var(--teal)' }}>a3f9c2d1-…</code>
                  Think of it as your API's passport number.
                </p>
              </Callout>
            </GuideStep>

            <GuideStep n={3} emoji="👥" title="Register your first Consumer">
              <p>
                Go to <strong style={{ color: 'var(--text-1)' }}>Consumers</strong> and click
                "Register Consumer". Fill in the name, owner team, and contact email. You can optionally
                enter a repository URL. Then subscribe the consumer to your new service using the pill buttons.
              </p>
              <p>
                Once added, the blast radius on any future diff will automatically include this consumer.
                No CLI required for this step.
              </p>
            </GuideStep>

            <GuideStep n={4} emoji="🔍" title="Run your first diff">
              <p>
                You have two options:
              </p>
              <p className="font-medium" style={{ color: 'var(--text-1)' }}>Option A — in the browser (no CLI needed):</p>
              <p>
                Go to <strong style={{ color: 'var(--text-1)' }}>Diffs</strong> and click the
                <strong style={{ color: 'var(--text-1)' }}> Compare Specs</strong> button in the toolbar.
                Paste or upload your "before" spec in the left panel and the "after" spec in the right panel.
                Click <em>Compare Specs</em> — Radar stores the diff and takes you straight to the detail view.
              </p>
              <p className="font-medium mt-2" style={{ color: 'var(--text-1)' }}>Option B — via the CLI (for automation):</p>
              <Block>{`radar check \\
  --base old-openapi.yaml \\
  --head new-openapi.yaml \\
  --service-id <your-service-id> \\
  --api-url http://localhost:8081`}</Block>
              <Callout variant="analogy">
                Running a diff is like asking "what's different between the menu from last week and today's menu?"
                Radar reads both spec files and produces a structured, colour-coded answer.
              </Callout>
            </GuideStep>

            <GuideStep n={5} emoji="📝" title="Generate release notes">
              <p>
                On the Diff detail page, click the
                <strong style={{ color: 'var(--cobalt-mid)' }}> Generate Release Notes</strong> button
                (sparkle icon, top-right of the header). Radar instantly produces a structured Markdown
                document: a summary, breaking changes with migration advice, risky changes, and safe changes.
                The result is saved to the <strong style={{ color: 'var(--text-1)' }}>Release Notes</strong> page
                as a draft.
              </p>
              <p>
                For AI-enhanced per-consumer migration guides, use the CLI:
                <code className="mx-1 text-[11px]" style={{ color: 'var(--teal)', fontFamily: 'var(--font-mono)' }}>
                  radar explain --diff-id … --migration-guide
                </code>
                (requires <Code>ANTHROPIC_API_KEY</Code> or another AI provider).
              </p>
            </GuideStep>

            <GuideStep n={6} emoji="🎯" title="Set up your pre-sales sandbox environment">
              <p>
                Go to <strong style={{ color: 'var(--text-1)' }}>API Playground</strong> in the sidebar.
                Open the <strong style={{ color: 'var(--text-1)' }}>Environments</strong> bar and click
                <em> New environment</em>.
              </p>
              <p>Fill in four fields:</p>
              <ul className="list-disc pl-5 space-y-1 text-[12.5px]">
                <li><strong style={{ color: 'var(--text-1)' }}>Name</strong> — something memorable, e.g. "Base Demo Tenant"</li>
                <li><strong style={{ color: 'var(--text-1)' }}>Description</strong> — a one-liner like "pre-sales sandbox, refreshed monthly"</li>
                <li><strong style={{ color: 'var(--text-1)' }}>Base URL</strong> — the URL of your demo environment, e.g. <code className="text-[11px]" style={{ color: 'var(--teal)' }}>https://sandbox.yourapi.com</code></li>
                <li><strong style={{ color: 'var(--text-1)' }}>Bearer token</strong> — your demo API key</li>
              </ul>
              <p>
                Click <em>Add</em>. The environment is saved to the server immediately — your colleagues
                will see it the next time they open the Playground, with no sharing required on your part.
                Click the chip to activate it, load your spec, and you're ready to demo.
              </p>
              <Callout variant="tip">
                When your demo API key rotates, just edit the environment and update the token.
                Every teammate gets the new token automatically — no "here's the new Postman collection" email.
              </Callout>
              <Callout variant="celebrate">
                Done! You have a full audit trail, shared release notes, <em>and</em> a zero-cost,
                zero-setup demo environment your whole team can use — all in one place.
              </Callout>
            </GuideStep>
          </div>
        </Card>
      </section>

      {/* ------------------------------------------------------------------ */}
      {/* FAQ                                                                   */}
      {/* ------------------------------------------------------------------ */}
      <section className="space-y-4">
        <h3 className="text-[15px] font-bold" style={{ color: 'var(--text-1)' }}>
          Chapter 4 — Frequently asked questions
        </h3>
        <Card className="!space-y-0">
          <FAQ q="Do I need to understand OpenAPI / YAML to use this?">
            <p>
              For the dashboard: <strong>no</strong>. You read the results — your developers produce the spec files.
              An OpenAPI spec is just a document (a YAML or JSON file) that describes an API.
              Your developers already have one if the API has any documentation at all.
            </p>
          </FAQ>
          <FAQ q="What's the difference between a breaking change and a non-breaking change?">
            <p>
              A <strong>breaking change</strong> is a modification that will cause existing consumers to fail
              without any code changes on their side. Examples: removing a field, renaming a field, changing a
              type from <Code>string</Code> to <Code>integer</Code>, or making an optional field required.
            </p>
            <p className="mt-2">
              A <strong>non-breaking change</strong> is one that existing consumers can safely ignore.
              Examples: adding a new optional field, adding a new endpoint, expanding an enum with a new value.
            </p>
          </FAQ>
          <FAQ q="What does 'blast radius' mean exactly?">
            <p>
              Blast radius is borrowed from the military — it's the zone of damage around an explosion.
              In Radar, it's the set of consumers that would be broken by a specific breaking change.
              A consumer is in the blast radius if it has been seen accessing the affected endpoint or
              field within the last 30 days (configurable).
            </p>
            <p className="mt-2">
              Radar distinguishes two levels of evidence. For <strong>operation-level</strong> changes
              (an endpoint removed or a parameter changed), any telemetry event for that operation
              counts. For <strong>field-level</strong> changes (a response property removed or its type
              changed), Radar requires telemetry that shows the consumer specifically accessed that
              field — so a consumer that calls <Code>GET /users</Code> but never reads the{' '}
              <Code>phone</Code> field will not appear in the blast radius for a{' '}
              <Code>GET /users → response.phone</Code> change.
            </p>
          </FAQ>
          <FAQ q="Does the AI read my API data or spec files?">
            <p>
              Only if you use the AI-powered features (Generate Tests, Release Notes migration guides).
              When you trigger those, the spec content is sent to the AI provider you've configured
              (Anthropic Claude, OpenAI, or GitHub Copilot). No data is sent automatically without you
              clicking "Generate". You can check which AI provider is connected in{' '}
              <strong>Settings → Integrations</strong>.
            </p>
          </FAQ>
          <FAQ q="Where is my data stored, and is the desktop app secure?">
            <p>
              In the <strong>desktop app</strong>, everything is stored locally in a database on
              your own machine — specs, diffs, evidence, and history never leave your computer
              unless you explicitly use an AI or notification integration. The app runs a local
              background service that is protected with a per-session token, so other apps or
              websites open on your machine cannot read or change your Radar data.
            </p>
            <p className="mt-2">
              In a <strong>self-hosted web deployment</strong>, data lives in the database your
              team configured, scoped per organization so one team never sees another's data.
              Secrets — bearer tokens, passwords, API keys — are redacted from logs and audit
              records.
            </p>
          </FAQ>
          <FAQ q="I don't have Jira. Can I still use Generate Tests?">
            <p>
              Yes. On the <strong>Generate Tests</strong> page, click "Switch to paste text" next to the
              Jira field. You can paste any description of what the API should do — even an informal
              bullet list — and the AI will generate test cases from it.
            </p>
          </FAQ>
          <FAQ q="What is the API Playground and is it safe to use in production?">
            <p>
              The Playground sends real HTTP requests to whatever base URL you configure — it's a live
              tool, not a sandbox. Create an Environment pointing at your staging or demo base URL
              to avoid accidentally hitting production. Bearer tokens are stored in the Radar database
              (encrypted at rest if your database is configured that way) and are never logged or
              included in telemetry.
            </p>
            <p className="mt-2">
              For pre-sales demos, the recommended pattern is one named environment per demo tenant,
              each with its own bearer token. Your whole team shares these environments automatically —
              no Postman workspace, no per-seat licence needed.
            </p>
          </FAQ>
          <FAQ q="The Environments bar says 'local only' with an amber icon. What does that mean?">
            <p>
              It means the Radar server was unreachable when the Playground loaded, so your environments
              are being stored in this browser only. Any environments you create will <strong>not</strong> be
              visible to teammates on other machines.
            </p>
            <p className="mt-2">
              This usually happens in two situations:
            </p>
            <ul className="list-disc pl-5 mt-1 space-y-1">
              <li>The <Code>radar-api</Code> server is not running — ask your developer to start it.</li>
              <li>You're using the desktop app and the sidecar hasn't started yet — wait a few seconds and reload.</li>
            </ul>
            <p className="mt-2">
              Once the server is reachable again, reload the page. The Playground will switch to shared
              mode automatically and load the team's environments from the database.
            </p>
          </FAQ>
          <FAQ q="What spec formats are supported?">
            <p>
              Radar supports three formats, auto-detected from the file extension:
            </p>
            <ul className="list-disc pl-5 mt-1 space-y-1">
              <li><strong>OpenAPI 3.x</strong> — <Code>.yaml</Code> or <Code>.json</Code> files (most REST APIs)</li>
              <li><strong>GraphQL SDL</strong> — <Code>.graphql</Code> schema definition files</li>
              <li><strong>Protobuf 3</strong> — <Code>.proto</Code> files (gRPC services)</li>
            </ul>
            <p className="mt-2">
              For OpenAPI, Radar resolves local <Code>$ref</Code> pointers (e.g.{' '}
              <Code>#/components/schemas/User</Code>) when comparing schemas, parameters, and
              responses. External file references are not followed; a warning is logged and that
              element is skipped.
            </p>
          </FAQ>
          <FAQ q="What is the CSV Runner and when should I use it?">
            <p>
              The CSV Runner (sidebar → <strong>CSV Runner</strong>) lets you send a batch of API
              calls from a spreadsheet — no code, no Postman, no scripting. Each row in your CSV
              becomes one HTTP request; the URL, body, and headers are built from a template you
              define once using <Code>{'{{column_name}}'}</Code> placeholders.
            </p>
            <p className="mt-2">
              Typical uses: smoke-testing a list of customer IDs, bulk-seeding a staging database,
              verifying that all endpoints in a changeset still return 200 after a deployment.
            </p>
          </FAQ>
          <FAQ q="What happens if the server returns a 503 mid-run?">
            <p>
              For HTTP 5xx responses and network errors, Radar automatically retries the row up to
              <strong> 3 times</strong>, waiting 1 second after the first retry and 4 seconds after
              the second. If all three attempts fail, the row is recorded as an error with the last
              HTTP status or error message.
            </p>
            <p className="mt-2">
              4xx responses (e.g. 400, 404, 422) are <strong>not</strong> retried — the server
              gave a definitive answer, and retrying would just repeat the same rejection.
            </p>
          </FAQ>
          <FAQ q="What does 'Capture response body' do in the CSV Runner?">
            <p>
              When you tick <strong>Capture response body</strong> before starting a run, Radar
              stores the first 10 KB of each row's response in the database alongside the HTTP
              status and timing.
            </p>
            <p className="mt-2">
              After the run completes, expand any result row to read the raw response inline —
              useful for debugging unexpected 4xx bodies or checking what was returned for a specific
              input row without re-running the entire batch. The body is stored per-row and survives
              page reloads.
            </p>
            <Callout variant="warning">
              Leave it unchecked for large batches if you don't need to inspect bodies — storing
              10 KB × hundreds of rows uses more database space than storing just the status.
            </Callout>
          </FAQ>
          <FAQ q="The breaking-change count seems high. Should I be worried?">
            <p>
              Not necessarily. A high count means your teams are moving fast. What matters is the
              <strong> blast radius</strong> on each change. A breaking change with zero active consumers
              is cosmetic — nobody will be affected. Only changes with active consumers inside the blast radius
              need immediate attention.
            </p>
            <p className="mt-2">
              You can also tune the policy (Settings → Default Policy) to decide whether the CI pipeline
              should <strong>block</strong> on any breaking change, only on ones with active consumers, or
              <strong> never</strong> block (warn only).
            </p>
          </FAQ>
        </Card>
      </section>

      {/* ------------------------------------------------------------------ */}
      {/* Glossary                                                              */}
      {/* ------------------------------------------------------------------ */}
      <section className="space-y-4">
        <h3 className="text-[15px] font-bold" style={{ color: 'var(--text-1)' }}>
          Quick Glossary
        </h3>
        <Card>
          <div className="grid gap-2" style={{ gridTemplateColumns: 'repeat(auto-fill, minmax(280px, 1fr))' }}>
            {[
              ['API', 'Application Programming Interface — the way two software services talk to each other.'],
              ['Contract', 'The agreed-on shape of an API: fields, types, and expected responses.'],
              ['Spec', 'A machine-readable document describing an API contract (usually an OpenAPI YAML file).'],
              ['Diff', 'The result of comparing two versions of a spec. Lists every change.'],
              ['Breaking Change', 'A change that will cause existing consumers to fail without code updates.'],
              ['Non-Breaking Change', 'An additive or compatible change that existing consumers can safely ignore.'],
              ['Producer', 'The service that owns and publishes an API spec.'],
              ['Consumer', 'The service that calls a Producer\'s API.'],
              ['Blast Radius', 'The set of active consumers that would break from a given breaking change.'],
              ['Call Site', 'A place in consumer code where a specific API field is accessed.'],
              ['Usage Event', 'A live telemetry record of a consumer accessing a field at a specific time.'],
              ['Migration Guide', 'Per-consumer AI-written instructions: what broke and how to fix it.'],
              ['Release Notes', 'A human-readable summary of all changes in a diff, for consumers to read.'],
              ['Policy', 'Rules in .radar.yml that control whether a diff blocks CI or only warns.'],
              ['Lookback Window', 'The rolling time window (default 30 days) used to decide if a consumer is "active".'],
              ['Playground', 'The embedded interactive API explorer (powered by Scalar). Replaces Postman for demos and connectivity checks.'],
              ['Sandbox Environment', 'A named base URL + bearer token pair saved in the shared database, used to pre-configure the Playground for demos. Visible to all teammates.'],
              ['CSV Runner', 'A bulk execution tool that reads a CSV file and fires one API request per row, substituting column values into a URL/method/body template. Supports opt-in response body capture and automatic 5xx retry.'],
            ].map(([term, def]) => (
              <div key={term} className="space-y-0.5">
                <span className="text-[12px] font-bold" style={{ color: 'var(--text-1)' }}>{term}</span>
                <p className="text-[11.5px] leading-relaxed" style={{ color: 'var(--text-3)' }}>{def}</p>
              </div>
            ))}
          </div>
        </Card>
      </section>
    </div>
  )
}

// ---------------------------------------------------------------------------
// CLI Reference tab (unchanged content, updated for multi-provider AI)
// ---------------------------------------------------------------------------

function CliReference() {
  return (
    <div className="space-y-10">

      <RefSection title="Quick Start — 5-step workflow" icon={Zap}>
        <Card>
          <div className="space-y-5">
            <Step n={1} title="Start the API server">
              <Block>{`radar-api --db sqlite:radar.db --bind 127.0.0.1:8081`}</Block>
              <p className="text-[12px]" style={{ color: 'var(--text-3)' }}>
                Or via Docker: <Code>docker compose up</Code> (uses PostgreSQL).
                Set <Code>RADAR_SERVICE_TOKEN</Code> to protect the API.
              </p>
            </Step>

            <Step n={2} title="Check a diff in CI">
              <Block>{`radar check \\
  --base old.yaml --head new.yaml \\
  --api-url http://localhost:8081 \\
  --service-id <uuid> \\
  --post-comment          # posts to GitHub PR`}</Block>
              <p className="text-[12px]" style={{ color: 'var(--text-3)' }}>
                Exits <Code>0</Code> (clean), <Code>1</Code> (breaking changes), or <Code>2</Code> (parse error).
                Format auto-detected from extension: <Code>.yaml</Code> → OpenAPI, <Code>.graphql</Code> → GraphQL, <Code>.proto</Code> → protobuf.
              </p>
            </Step>

            <Step n={3} title="Register consumers">
              <Block>{`radar register \\
  --api-url http://localhost:8081 \\
  --service-id <producer-uuid> \\
  --consumer-name checkout-svc \\
  --repo-url https://github.com/org/checkout \\
  --owner-team payments \\
  --contact ops@example.com`}</Block>
            </Step>

            <Step n={4} title="Scan consumer repos for call sites (optional)">
              <Block>{`radar scan \\
  --consumer-id <uuid> \\
  --service-id <uuid> \\
  --source-dir ./checkout-svc \\
  --api-url http://localhost:8081 \\
  --operation-map "userId=GET /users" \\
  --operation-map "email=GET /users"`}</Block>
              <p className="text-[12px]" style={{ color: 'var(--text-3)' }}>
                Uses tree-sitter to find field accesses in TypeScript, Python, and Go.
                Use <Code>--operation-map field=METHOD /path</Code> to tie fields to concrete API
                operations for richer blast-radius evidence. Omit to fall back to field-path-only matching.
              </p>
            </Step>

            <Step n={5} title="Generate release notes or Postman tests">
              <Block>{`# Markdown release notes with per-consumer migration guides
radar explain \\
  --diff-id <uuid> --api-url http://localhost:8081 \\
  --release-notes --migration-guide \\
  --out RELEASE_NOTES.md

# Postman test collection from a Jira ticket
radar generate-tests \\
  --jira PROJ-123 --spec openapi.yaml \\
  --out tests.postman_collection.json \\
  --out-apitesting tests.api-testing.yaml`}</Block>
            </Step>
          </div>
        </Card>
      </RefSection>

      <RefSection title="CLI Command Reference" icon={Terminal}>
        <div className="space-y-4">
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

          <Card>
            <p className="text-[12.5px] font-semibold" style={{ color: 'var(--text-1)' }}>
              <Code>radar register</Code> — Register a consumer and subscribe it to a producer
            </p>
            <div className="space-y-1.5 pt-1">
              <Flag name="--api-url <url>"        desc="radar-api base URL" />
              <Flag name="--service-id <uuid>"    desc="Producer service UUID to subscribe to" />
              <Flag name="--consumer-name <name>" desc="Display name for this consumer" />
              <Flag name="--repo-url <url>"       desc="Repository URL of the consumer service" />
              <Flag name="--owner-team <team>"    desc="Team name responsible for this consumer" />
              <Flag name="--contact <email>"      desc="Contact address for blast-radius notifications" />
              <Flag name="--token <token>"        desc="Bearer token for the API" />
            </div>
          </Card>

          <Card>
            <p className="text-[12.5px] font-semibold" style={{ color: 'var(--text-1)' }}>
              <Code>radar scan</Code> — Extract API call sites from consumer source code
            </p>
            <div className="space-y-1.5 pt-1">
              <Flag name="--consumer-id <uuid>"       desc="UUID of the consumer to scan" />
              <Flag name="--service-id <uuid>"        desc="UUID of the producer whose fields to track" />
              <Flag name="--source-dir <path>"        desc="Root directory to scan (TypeScript, Python, Go)" />
              <Flag name="--api-url <url>"            desc="radar-api base URL" />
              <Flag name="--token <token>"            desc="Bearer token for the API" />
              <Flag name="--operation-map field=OP"   desc='Map a field name to an API operation e.g. "userId=GET /users". Repeat for multiple fields.' />
            </div>
            <p className="text-[11.5px] pt-1" style={{ color: 'var(--text-3)' }}>
              Uses tree-sitter grammars. Results posted to <Code>POST /v1/call-sites</Code> in batches of 500.
            </p>
          </Card>

          <Card>
            <p className="text-[12.5px] font-semibold" style={{ color: 'var(--text-1)' }}>
              <Code>radar explain</Code> — Generate release notes and migration guides for a diff
            </p>
            <div className="space-y-1.5 pt-1">
              <Flag name="--diff-id <uuid>"       desc="UUID of the diff to explain (shown in the Diffs page)" />
              <Flag name="--api-url <url>"         desc="radar-api base URL" />
              <Flag name="--release-notes"         desc="Generate Markdown release notes to stdout or --out file" />
              <Flag name="--migration-guide"       desc="Add per-consumer migration guides via AI (needs ANTHROPIC_API_KEY, OPENAI_API_KEY, or GITHUB_COPILOT_TOKEN)" />
              <Flag name="--post-github-release"   desc="Create a GitHub Release with the notes (needs GITHUB_TOKEN)" />
              <Flag name="--out <path>"            desc="Write output to a file instead of stdout" />
            </div>
          </Card>

          <Card>
            <p className="text-[12.5px] font-semibold" style={{ color: 'var(--text-1)' }}>
              <Code>radar generate-tests</Code> — Generate a Postman Collection from a Jira ticket + spec
            </p>
            <div className="space-y-1.5 pt-1">
              <Flag name="--jira <key>"              desc="Jira ticket key e.g. PROJ-123 (uses JIRA_BASE_URL / JIRA_EMAIL / JIRA_TOKEN)" />
              <Flag name={'--jira-text "..."'}         desc="Paste ticket text directly (fallback when Jira credentials are absent)" />
              <Flag name="--spec <path>"             desc="Path to the OpenAPI YAML/JSON spec file (required)" />
              <Flag name="--base-url <url>"          desc="API base URL inserted into every generated request (default: http://localhost:8080)" />
              <Flag name="--out <path>"              desc="Write the Postman Collection JSON to a file" />
              <Flag name="--out-apitesting <path>"   desc="Also write an api-testing YAML suite (LinuxSuRen/api-testing format)" />
              <Flag name="--postman-workspace <id>"  desc="Push the collection to this Postman workspace (needs POSTMAN_API_KEY)" />
            </div>
            <p className="text-[11.5px] pt-1" style={{ color: 'var(--text-3)' }}>
              Requires one AI provider: <Code>ANTHROPIC_API_KEY</Code>, <Code>OPENAI_API_KEY</Code>, or <Code>GITHUB_COPILOT_TOKEN</Code> (first configured wins).
            </p>
          </Card>

          <Card>
            <p className="text-[12.5px] font-semibold" style={{ color: 'var(--text-1)' }}>
              <Code>radar rule</Code> — Manage evolution rules (allow-list expected changes)
            </p>
            <div className="space-y-1.5 pt-1">
              <Flag name="add"              desc="Create a rule: --name, --change-kind, --severity-override (safe|non_breaking_risky), --path-pattern (optional glob), --api-url" />
              <Flag name="list"             desc="List all evolution rules for this org (--api-url)" />
              <Flag name="delete <id>"      desc="Permanently delete a rule by ID" />
              <Flag name="toggle <id>"      desc="Enable or disable a rule: --enabled true|false" />
              <Flag name="test"             desc="Show which rules would apply to a specific diff: --diff-id <uuid>" />
            </div>
            <p className="text-[11.5px] pt-1" style={{ color: 'var(--text-3)' }}>
              An active rule downgrades a specific <Code>change_kind</Code> to the
              target <Code>severity_override</Code>, preventing a CI block for a planned change.
            </p>
          </Card>

          <Card>
            <p className="text-[12.5px] font-semibold" style={{ color: 'var(--text-1)' }}>
              <Code>radar batch</Code> — Compare multiple spec pairs listed in a CSV file
            </p>
            <div className="space-y-1.5 pt-1">
              <Flag name="--csv <path>"     desc="CSV file with columns: base, head (required); label, format, service_id (optional)" />
              <Flag name="--api-url <url>"  desc="radar-api base URL (optional; enables posting results per-service)" />
              <Flag name="--token <token>"  desc="Bearer token for the API" />
              <Flag name="--json"           desc="Emit results as JSON instead of a colour table" />
              <Flag name="--no-color"       desc="Disable ANSI colour in table output" />
            </div>
            <Block>{`# Example CSV
base,head,label,service_id
old-payments.yaml,new-payments.yaml,payments,abc-123
old-orders.yaml,new-orders.yaml,orders,`}</Block>
            <p className="text-[11.5px] pt-1" style={{ color: 'var(--text-3)' }}>
              Each row is compared independently. Results are printed as a table (one row per pair)
              or as a JSON array when <Code>--json</Code> is passed.
            </p>
          </Card>

          <Card>
            <p className="text-[12.5px] font-semibold" style={{ color: 'var(--text-1)' }}>
              <Code>radar completions &lt;shell&gt;</Code> — Print shell completion script to stdout
            </p>
            <p className="text-[12.5px] pt-1" style={{ color: 'var(--text-3)' }}>
              Supported: <Code>bash</Code>, <Code>zsh</Code>, <Code>fish</Code>, <Code>powershell</Code>, <Code>elvish</Code>
            </p>
            <Block>{`radar completions bash >> ~/.bash_completion
radar completions zsh > ~/.oh-my-zsh/completions/_radar
radar completions fish > ~/.config/fish/completions/radar.fish`}</Block>
          </Card>
        </div>
      </RefSection>

      <RefSection title="Policy File (.radar.yml)" icon={FileText}>
        <Card>
          <p className="text-[12.5px]" style={{ color: 'var(--text-2)' }}>
            Place a <Code>.radar.yml</Code> in your repo root to control CI behaviour without changing the command.
          </p>
          <Block>{`policy:
  # never | any_break | active_consumers (default)
  block_on: active_consumers

  # Allow a PR to pass even with breaking changes if it carries this label.
  allow_override_with: "label:radar-ack"`}</Block>
          <div className="space-y-1.5 text-[12.5px]">
            <Flag name="block_on: never"            desc="Never block CI — warnings only" />
            <Flag name="block_on: any_break"        desc="Block on any breaking change (exit 1)" />
            <Flag name="block_on: active_consumers" desc="Block only when at least one subscribed consumer is active (default)" />
            <Flag name="allow_override_with: ..."   desc="label:radar-ack — GitHub label escape hatch for approved exceptions" />
          </div>
        </Card>
      </RefSection>

      <RefSection title="Environment Variables" icon={Key}>
        <Card>
          <div className="divide-y" style={{ borderColor: 'var(--border)' }}>
            <div className="pb-2">
              <p className="text-[10.5px] font-semibold uppercase tracking-wider pb-2" style={{ color: 'var(--text-dim)' }}>Core API</p>
              <EnvVar name="RADAR_API_URL"        desc="Base URL of the radar-api server used by the CLI" />
              <EnvVar name="RADAR_SERVICE_TOKEN"  desc="Static bearer token that the CLI sends and the server validates" />
              <EnvVar name="RADAR_JWT_SECRET"     desc="HS256 JWT secret on the server (overrides static token auth)" />
              <EnvVar name="RADAR_REQUIRE_AUTH"   desc="Set to true or 1 to reject unauthenticated /v1/* requests even when no token is configured (production lockdown)" />
            </div>
            <div className="py-2">
              <p className="text-[10.5px] font-semibold uppercase tracking-wider pb-2 pt-3" style={{ color: 'var(--text-dim)' }}>AI Providers (first configured wins)</p>
              <EnvVar name="ANTHROPIC_API_KEY"    desc="Anthropic Claude — highest priority AI provider" />
              <EnvVar name="OPENAI_API_KEY"       desc="OpenAI GPT-4o — second priority" />
              <EnvVar name="OPENAI_BASE_URL"      desc="Custom base URL for ChatGPT Enterprise / Azure OpenAI (optional, used with OPENAI_API_KEY)" />
              <EnvVar name="GITHUB_COPILOT_TOKEN" desc="GitHub Copilot — third priority" />
            </div>
            <div className="py-2">
              <p className="text-[10.5px] font-semibold uppercase tracking-wider pb-2 pt-3" style={{ color: 'var(--text-dim)' }}>Integrations</p>
              <EnvVar name="JIRA_BASE_URL"        desc="Your Jira Cloud base URL (e.g. https://mycompany.atlassian.net)" />
              <EnvVar name="JIRA_EMAIL"           desc="Atlassian account email for Jira Basic auth" />
              <EnvVar name="JIRA_TOKEN"           desc="Atlassian API token for Jira Basic auth" />
              <EnvVar name="POSTMAN_API_KEY"      desc="Postman API key for --postman-workspace push" />
            </div>
            <div className="py-2">
              <p className="text-[10.5px] font-semibold uppercase tracking-wider pb-2 pt-3" style={{ color: 'var(--text-dim)' }}>GitHub (CI)</p>
              <EnvVar name="GITHUB_TOKEN"         desc="Required for --post-comment and --post-github-release" />
              <EnvVar name="GITHUB_REPOSITORY"    desc="Set automatically by GitHub Actions (owner/repo)" />
              <EnvVar name="GITHUB_REF"           desc="Set automatically by GitHub Actions — used to extract the PR number" />
              <EnvVar name="GITHUB_PR_NUMBER"     desc="Explicit PR number override for --post-comment (takes priority over GITHUB_REF and GITHUB_EVENT_PATH)" />
              <EnvVar name="GITHUB_EVENT_PATH"    desc="Set automatically by GitHub Actions — path to the event JSON file; used to extract the PR number when GITHUB_PR_NUMBER is absent" />
              <EnvVar name="NO_COLOR"             desc="Disable ANSI colour output in radar CLI (standard convention; also honoured by --no-color flag)" />
            </div>
            <div className="py-2">
              <p className="text-[10.5px] font-semibold uppercase tracking-wider pb-2 pt-3" style={{ color: 'var(--text-dim)' }}>Security</p>
              <EnvVar name="RADAR_ALLOWED_HOSTS"    desc="Glob-pattern allowlist for outbound HTTP (webhooks, scans, CSV runner). Comma-separated, e.g. *.example.com,api.partner.io. Default: all hosts allowed." />
            </div>
            <div className="py-2">
              <p className="text-[10.5px] font-semibold uppercase tracking-wider pb-2 pt-3" style={{ color: 'var(--text-dim)' }}>Server tuning</p>
              <EnvVar name="RATE_LIMIT_PER_MINUTE"     desc="Max requests per IP per minute (default 300, 0 = unlimited)" />
              <EnvVar name="MAX_BODY_SIZE_MB"         desc="Maximum request body size in megabytes (default 4 MB)" />
              <EnvVar name="RADAR_REQUEST_TIMEOUT_SECS" desc="Request timeout in seconds for all routes (default 30)" />
              <EnvVar name="CORS_ALLOWED_ORIGINS"    desc="Comma-separated allowed CORS origins. Omit for permissive (dev)." />
              <EnvVar name="DATABASE_URL"            desc="sqlx connection string: sqlite:radar.db or postgres://user:pass@host/db" />
              <EnvVar name="BIND_ADDR"               desc="Socket address to listen on (default 0.0.0.0:8081)" />
            </div>
          </div>
        </Card>
      </RefSection>

      <RefSection title="Generate Postman Tests — feature walkthrough" icon={FlaskConical}>
        <Card>
          <p className="text-[12.5px]" style={{ color: 'var(--text-2)' }}>
            Radar uses an AI provider to read a Jira ticket and your OpenAPI spec and produce a
            Postman Collection v2.1 and an api-testing YAML suite with happy-path and negative test cases.
          </p>
          <div className="space-y-3 pt-1">
            <p className="text-[12px] font-semibold" style={{ color: 'var(--text-1)' }}>Option A — via the CLI</p>
            <Block>{`export JIRA_BASE_URL=https://mycompany.atlassian.net
export JIRA_EMAIL=you@example.com
export JIRA_TOKEN=<atlassian-api-token>
export ANTHROPIC_API_KEY=<key>   # or OPENAI_API_KEY / GITHUB_COPILOT_TOKEN

radar generate-tests \\
  --jira PROJ-123 \\
  --spec docs/openapi.yaml \\
  --base-url https://api.example.com \\
  --out PROJ-123.postman_collection.json \\
  --out-apitesting PROJ-123.api-testing.yaml`}</Block>

            <p className="text-[12px] font-semibold" style={{ color: 'var(--text-1)' }}>Option B — via the dashboard</p>
            <p className="text-[12.5px]" style={{ color: 'var(--text-2)' }}>
              Go to <strong>Testing → Generate Tests</strong>. Paste the Jira key (or ticket text), paste the spec YAML,
              set the base URL, and click Generate. Download as Postman JSON or api-testing YAML.
            </p>

            <p className="text-[12px] font-semibold" style={{ color: 'var(--text-1)' }}>What the AI generates</p>
            <p className="text-[12.5px]" style={{ color: 'var(--text-2)' }}>
              4–6 happy-path tests covering the ticket's acceptance criteria, and 4–8 negative tests
              (missing required fields → 422, wrong types → 400, unauthorized → 401, not-found → 404).
              Each item has a <Code>pm.test()</Code> assertion block. <Code>{'{{baseUrl}}'}</Code> and
              <Code>{'{{authToken}}'}</Code> are collection variables set once per environment.
            </p>
          </div>
        </Card>
      </RefSection>
    </div>
  )
}

// ---------------------------------------------------------------------------
// Page root
// ---------------------------------------------------------------------------

export default function HelpPage() {
  const [tab, setTab] = useState<Tab>('guide')

  return (
    <div className="p-8 max-w-4xl mx-auto space-y-8">
      <PageHeader
        tag="Help"
        title="Help & Reference"
        description="Beginner's guide, CLI command reference, and environment variable documentation"
      />

      <TabBar active={tab} onChange={setTab} />

      {tab === 'guide' ? <BeginnersGuide /> : <CliReference />}
    </div>
  )
}
