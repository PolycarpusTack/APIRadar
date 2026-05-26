import { useEffect, useState } from 'react'
import { X, UserPlus } from 'lucide-react'
import { api, ApiError } from '../lib/apiClient'

interface Service {
  id: string
  name: string
}

interface ConsumerRow {
  id: string
  name: string
  repo_url: string
  owner_team: string
  contact: string
}

interface Props {
  onCreated: (consumer: ConsumerRow) => void
  onClose: () => void
}

interface FieldErrors {
  name?: string
  owner_team?: string
  contact?: string
}

export default function RegisterConsumerForm({ onCreated, onClose }: Props) {
  const [services, setServices] = useState<Service[]>([])
  const [name, setName] = useState('')
  const [ownerTeam, setOwnerTeam] = useState('')
  const [contact, setContact] = useState('')
  const [repoUrl, setRepoUrl] = useState('')
  const [selectedServiceIds, setSelectedServiceIds] = useState<string[]>([])
  const [saving, setSaving] = useState(false)
  const [fieldErrors, setFieldErrors] = useState<FieldErrors>({})
  const [apiError, setApiError] = useState<string | null>(null)

  useEffect(() => {
    api.get<Service[]>('/v1/services')
      .then(setServices)
      .catch(() => {})
  }, [])

  function toggleService(id: string) {
    setSelectedServiceIds((prev) =>
      prev.includes(id) ? prev.filter((s) => s !== id) : [...prev, id]
    )
  }

  function validate(): boolean {
    const errors: FieldErrors = {}
    if (!name.trim()) errors.name = 'Consumer name is required'
    if (!ownerTeam.trim()) errors.owner_team = 'Owner team is required'
    if (!contact.trim()) errors.contact = 'Contact email is required'
    setFieldErrors(errors)
    return Object.keys(errors).length === 0
  }

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault()
    setApiError(null)
    if (!validate()) return

    setSaving(true)
    try {
      const consumer = await api.post<ConsumerRow>('/v1/consumers', {
        name: name.trim(),
        repo_url: repoUrl.trim(),
        owner_team: ownerTeam.trim(),
        contact: contact.trim(),
      })

      // Subscribe to each selected service — fire-and-forget per service.
      await Promise.all(
        selectedServiceIds.map((svcId) =>
          api.post(`/v1/services/${encodeURIComponent(svcId)}/subscriptions`, { consumer_id: consumer.id })
            .catch(() => {})
        )
      )

      onCreated(consumer)
    } catch (err) {
      if (err instanceof ApiError && err.status === 409) {
        setApiError('A consumer with this name already exists')
      } else {
        setApiError((err as Error).message)
      }
    } finally {
      setSaving(false)
    }
  }

  return (
    <div
      className="rounded-lg border p-5 mb-6"
      style={{ border: '1px solid var(--border)', background: 'var(--bg-surface)' }}
    >
      <div className="flex items-center justify-between mb-4">
        <div className="flex items-center gap-2">
          <UserPlus className="h-4 w-4" style={{ color: 'var(--cobalt-mid)' }} />
          <p className="text-[12.5px] font-semibold" style={{ color: 'var(--text-1)' }}>
            Register a Consumer
          </p>
        </div>
        <button onClick={onClose} style={{ color: 'var(--text-3)' }}>
          <X className="h-4 w-4" />
        </button>
      </div>

      <form onSubmit={handleSubmit} className="flex flex-col gap-4">
        <div className="grid grid-cols-2 gap-3">
          {/* Consumer name */}
          <div>
            <label
              className="block text-[10.5px] font-semibold uppercase tracking-[0.8px] mb-1"
              style={{ color: fieldErrors.name ? 'var(--red)' : 'var(--text-3)' }}
            >
              Consumer Name *
            </label>
            <input
              value={name}
              onChange={(e) => { setName(e.target.value); setFieldErrors((p) => ({ ...p, name: undefined })) }}
              placeholder="e.g. billing-service"
              className="w-full rounded-md px-3 py-2 text-[12.5px]"
              style={{
                background: 'var(--bg-input, var(--bg-raised))',
                border: `1px solid ${fieldErrors.name ? 'var(--red)' : 'var(--border)'}`,
                color: 'var(--text-1)',
              }}
            />
            {fieldErrors.name && (
              <p className="mt-1 text-[11px]" style={{ color: 'var(--red)' }}>
                {fieldErrors.name}
              </p>
            )}
          </div>

          {/* Owner team */}
          <div>
            <label
              className="block text-[10.5px] font-semibold uppercase tracking-[0.8px] mb-1"
              style={{ color: fieldErrors.owner_team ? 'var(--red)' : 'var(--text-3)' }}
            >
              Owner Team *
            </label>
            <input
              value={ownerTeam}
              onChange={(e) => { setOwnerTeam(e.target.value); setFieldErrors((p) => ({ ...p, owner_team: undefined })) }}
              placeholder="e.g. Platform Team"
              className="w-full rounded-md px-3 py-2 text-[12.5px]"
              style={{
                background: 'var(--bg-input, var(--bg-raised))',
                border: `1px solid ${fieldErrors.owner_team ? 'var(--red)' : 'var(--border)'}`,
                color: 'var(--text-1)',
              }}
            />
            {fieldErrors.owner_team && (
              <p className="mt-1 text-[11px]" style={{ color: 'var(--red)' }}>
                {fieldErrors.owner_team}
              </p>
            )}
          </div>

          {/* Contact email */}
          <div>
            <label
              className="block text-[10.5px] font-semibold uppercase tracking-[0.8px] mb-1"
              style={{ color: fieldErrors.contact ? 'var(--red)' : 'var(--text-3)' }}
            >
              Contact Email *
            </label>
            <input
              type="email"
              value={contact}
              onChange={(e) => { setContact(e.target.value); setFieldErrors((p) => ({ ...p, contact: undefined })) }}
              placeholder="team@company.com"
              className="w-full rounded-md px-3 py-2 text-[12.5px]"
              style={{
                background: 'var(--bg-input, var(--bg-raised))',
                border: `1px solid ${fieldErrors.contact ? 'var(--red)' : 'var(--border)'}`,
                color: 'var(--text-1)',
              }}
            />
            {fieldErrors.contact && (
              <p className="mt-1 text-[11px]" style={{ color: 'var(--red)' }}>
                {fieldErrors.contact}
              </p>
            )}
          </div>

          {/* Repo URL — optional */}
          <div>
            <label
              className="block text-[10.5px] font-semibold uppercase tracking-[0.8px] mb-1"
              style={{ color: 'var(--text-3)' }}
            >
              Repository URL
              <span className="ml-1 font-normal normal-case" style={{ color: 'var(--text-dim)' }}>
                (optional)
              </span>
            </label>
            <input
              value={repoUrl}
              onChange={(e) => setRepoUrl(e.target.value)}
              placeholder="https://github.com/org/repo"
              className="w-full rounded-md px-3 py-2 text-[12.5px]"
              style={{
                background: 'var(--bg-input, var(--bg-raised))',
                border: '1px solid var(--border)',
                color: 'var(--text-1)',
                fontFamily: 'var(--font-mono)',
              }}
            />
          </div>
        </div>

        {/* Service subscriptions */}
        {services.length > 0 && (
          <div>
            <label
              className="block text-[10.5px] font-semibold uppercase tracking-[0.8px] mb-2"
              style={{ color: 'var(--text-3)' }}
            >
              Subscribe to Services
              <span className="ml-1 font-normal normal-case" style={{ color: 'var(--text-dim)' }}>
                (optional — you can add subscriptions later)
              </span>
            </label>
            <div className="flex flex-wrap gap-2">
              {services.map((svc) => {
                const selected = selectedServiceIds.includes(svc.id)
                return (
                  <button
                    key={svc.id}
                    type="button"
                    onClick={() => toggleService(svc.id)}
                    className="rounded-full px-3 py-1 text-[11.5px] font-medium transition-colors"
                    style={{
                      background: selected ? 'var(--cobalt-mid)' : 'var(--bg-raised)',
                      border: `1px solid ${selected ? 'var(--cobalt-mid)' : 'var(--border)'}`,
                      color: selected ? '#fff' : 'var(--text-2)',
                    }}
                  >
                    {svc.name}
                  </button>
                )
              })}
            </div>
          </div>
        )}

        {/* API error */}
        {apiError && (
          <p
            className="rounded-md px-3 py-2 text-[12px]"
            style={{ background: 'var(--red-bg)', border: '1px solid var(--red-dim)', color: 'var(--red)' }}
          >
            {apiError}
          </p>
        )}

        <div className="flex justify-end gap-2">
          <button
            type="button"
            onClick={onClose}
            className="btn-ghost rounded-md px-4 py-2 text-[12px] font-medium"
          >
            Cancel
          </button>
          <button
            type="submit"
            disabled={saving}
            className="btn-primary flex items-center gap-2 rounded-md px-5 py-2 text-[12.5px] font-medium"
          >
            <UserPlus className="h-3.5 w-3.5" />
            {saving ? 'Registering…' : 'Register'}
          </button>
        </div>
      </form>
    </div>
  )
}
