import { useEffect, useRef, useState } from 'react'
import { useNavigate, useSearchParams } from 'react-router-dom'
import { GitCompare, Upload, X, AlertCircle } from 'lucide-react'

interface Service {
  id: string
  name: string
  spec_format: string
}

interface CompareResult {
  diff_id: string
  changes_count: number
  breaking_count: number
}

interface ParseError {
  error: string
  detail: string
  spec: 'base' | 'head'
}

const FORMAT_OPTIONS = [
  { value: 'openapi', label: 'OpenAPI / Swagger' },
  { value: 'graphql', label: 'GraphQL SDL' },
  { value: 'protobuf', label: 'Protocol Buffers' },
]

function SpecTextarea({
  label,
  hint,
  value,
  onChange,
  hasError,
  fileInputId,
}: {
  label: string
  hint: string
  value: string
  onChange: (v: string) => void
  hasError: boolean
  fileInputId: string
}) {
  const fileRef = useRef<HTMLInputElement>(null)

  function handleFile(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0]
    if (!file) return
    const reader = new FileReader()
    reader.onload = () => onChange(reader.result as string)
    reader.readAsText(file)
    e.target.value = ''
  }

  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex items-center justify-between">
        <label
          className="text-[10.5px] font-semibold uppercase tracking-[0.8px]"
          style={{ color: hasError ? 'var(--red)' : 'var(--text-3)' }}
        >
          {label}
          {hasError && (
            <span className="ml-2 normal-case font-normal text-[11px]">
              ← parse error
            </span>
          )}
        </label>
        <button
          type="button"
          onClick={() => fileRef.current?.click()}
          className="flex items-center gap-1 text-[11px] rounded px-2 py-0.5 transition-colors hover:bg-[var(--bg-hover)]"
          style={{ color: 'var(--text-3)', border: '1px solid var(--border)' }}
        >
          <Upload className="h-3 w-3" />
          Load file
        </button>
      </div>
      <input
        id={fileInputId}
        ref={fileRef}
        type="file"
        accept=".yaml,.yml,.json,.graphql,.gql,.proto"
        className="hidden"
        onChange={handleFile}
      />
      <textarea
        value={value}
        onChange={(e) => onChange(e.target.value)}
        rows={14}
        spellCheck={false}
        placeholder={hint}
        className="w-full rounded-md p-3 text-[11.5px] leading-relaxed resize-y transition-colors"
        style={{
          fontFamily: 'var(--font-mono)',
          background: 'var(--bg-input, var(--bg-raised))',
          border: `1px solid ${hasError ? 'var(--red)' : 'var(--border)'}`,
          color: 'var(--text-1)',
          outline: 'none',
        }}
      />
    </div>
  )
}

export default function CompareSpecsPanel({ onClose }: { onClose?: () => void }) {
  const navigate = useNavigate()
  const [searchParams] = useSearchParams()
  const [services, setServices] = useState<Service[]>([])
  const [serviceId, setServiceId] = useState('')
  const [format, setFormat] = useState('openapi')
  const [baseSpec, setBaseSpec] = useState('')
  const [headSpec, setHeadSpec] = useState('')
  const [submitting, setSubmitting] = useState(false)
  const [parseError, setParseError] = useState<ParseError | null>(null)
  const [generalError, setGeneralError] = useState<string | null>(null)

  const preselectedId = searchParams.get('service_id') ?? ''

  useEffect(() => {
    fetch('/v1/services')
      .then((r) => r.json() as Promise<Service[]>)
      .then((list) => {
        setServices(list)
        const match = preselectedId && list.find((s) => s.id === preselectedId)
        if (match) {
          setServiceId(match.id)
          setFormat(match.spec_format)
        } else if (list.length > 0) {
          setServiceId(list[0].id)
        }
      })
      .catch(() => {})
  }, [])

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault()
    setParseError(null)
    setGeneralError(null)

    if (!serviceId) {
      setGeneralError('Select a service to compare against.')
      return
    }
    if (!baseSpec.trim()) {
      setGeneralError('Paste or upload the "before" spec.')
      return
    }
    if (!headSpec.trim()) {
      setGeneralError('Paste or upload the "after" spec.')
      return
    }

    setSubmitting(true)
    try {
      const resp = await fetch(`/v1/services/${encodeURIComponent(serviceId)}/diffs/compare`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          base_spec: baseSpec,
          head_spec: headSpec,
          spec_format: format,
          base_ref: 'before',
          head_ref: 'after',
        }),
      })

      if (resp.status === 422) {
        const body = await resp.json() as ParseError
        setParseError(body)
        return
      }
      if (!resp.ok) {
        const body = await resp.json().catch(() => ({})) as { error?: string }
        setGeneralError(body.error ?? `Unexpected error (HTTP ${resp.status})`)
        return
      }

      const result = await resp.json() as CompareResult
      navigate(`/diffs/${result.diff_id}`)
    } catch (err) {
      setGeneralError((err as Error).message)
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <div
      className="rounded-lg border p-6"
      style={{ background: 'var(--bg-surface)', border: '1px solid var(--border)' }}
    >
      <div className="flex items-center justify-between mb-5">
        <div className="flex items-center gap-2">
          <GitCompare className="h-4 w-4" style={{ color: 'var(--cobalt-mid)' }} />
          <p className="text-[13px] font-semibold" style={{ color: 'var(--text-1)' }}>
            Compare Two Specs
          </p>
        </div>
        {onClose && (
          <button onClick={onClose} style={{ color: 'var(--text-3)' }}>
            <X className="h-4 w-4" />
          </button>
        )}
      </div>

      <form onSubmit={handleSubmit} className="flex flex-col gap-5">
        {/* Service + format row */}
        <div className="grid grid-cols-2 gap-4">
          <div className="flex flex-col gap-1.5">
            <label
              className="text-[10.5px] font-semibold uppercase tracking-[0.8px]"
              style={{ color: 'var(--text-3)' }}
            >
              Producer Service *
            </label>
            {services.length === 0 ? (
              <p className="text-[11.5px]" style={{ color: 'var(--text-dim)' }}>
                No services registered yet — go to Services and add one first.
              </p>
            ) : (
              <select
                value={serviceId}
                onChange={(e) => setServiceId(e.target.value)}
                className="rounded-md px-3 py-2 text-[12.5px] transition-colors"
                style={{
                  background: 'var(--bg-input, var(--bg-raised))',
                  border: '1px solid var(--border)',
                  color: 'var(--text-1)',
                }}
              >
                {services.map((s) => (
                  <option key={s.id} value={s.id}>
                    {s.name}
                  </option>
                ))}
              </select>
            )}
          </div>

          <div className="flex flex-col gap-1.5">
            <label
              className="text-[10.5px] font-semibold uppercase tracking-[0.8px]"
              style={{ color: 'var(--text-3)' }}
            >
              Spec Format *
            </label>
            <select
              value={format}
              onChange={(e) => setFormat(e.target.value)}
              className="rounded-md px-3 py-2 text-[12.5px] transition-colors"
              style={{
                background: 'var(--bg-input, var(--bg-raised))',
                border: '1px solid var(--border)',
                color: 'var(--text-1)',
              }}
            >
              {FORMAT_OPTIONS.map((o) => (
                <option key={o.value} value={o.value}>
                  {o.label}
                </option>
              ))}
            </select>
          </div>
        </div>

        {/* Spec textareas */}
        <div className="grid grid-cols-2 gap-4">
          <SpecTextarea
            label="Before (old version)"
            hint={`Paste your ${format === 'openapi' ? 'OpenAPI YAML/JSON' : format === 'graphql' ? 'GraphQL SDL' : '.proto'} here, or use "Load file" above`}
            value={baseSpec}
            onChange={setBaseSpec}
            hasError={parseError?.spec === 'base'}
            fileInputId="base-spec-file"
          />
          <SpecTextarea
            label="After (new version)"
            hint="Paste the updated spec here to see what changed"
            value={headSpec}
            onChange={setHeadSpec}
            hasError={parseError?.spec === 'head'}
            fileInputId="head-spec-file"
          />
        </div>

        {/* Parse error detail */}
        {parseError && (
          <div
            className="flex items-start gap-2 rounded-md px-3 py-2.5 text-[12px]"
            style={{ background: 'var(--red-bg)', border: '1px solid var(--red-dim)', color: 'var(--red)' }}
          >
            <AlertCircle className="h-4 w-4 mt-0.5 flex-shrink-0" />
            <div>
              <span className="font-semibold capitalize">{parseError.spec} spec parse error: </span>
              <span className="font-mono text-[11px]">{parseError.detail}</span>
            </div>
          </div>
        )}

        {/* General error */}
        {generalError && (
          <div
            className="rounded-md px-3 py-2.5 text-[12px]"
            style={{ background: 'var(--red-bg)', border: '1px solid var(--red-dim)', color: 'var(--red)' }}
          >
            {generalError}
          </div>
        )}

        <div className="flex justify-end">
          <button
            type="submit"
            disabled={submitting || services.length === 0}
            className="flex items-center gap-2 rounded-md px-5 py-2 text-[12.5px] font-medium transition-opacity disabled:opacity-50"
            style={{ background: 'var(--cobalt-mid)', color: '#fff' }}
          >
            <GitCompare className="h-3.5 w-3.5" />
            {submitting ? 'Comparing…' : 'Compare Specs'}
          </button>
        </div>
      </form>
    </div>
  )
}
