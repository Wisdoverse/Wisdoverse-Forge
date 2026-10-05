import { useEffect, useRef, useState } from 'react'
import { deliveryGuidance } from './guidance'

/** Reuse the reference for an unchanged retry, including a lost HTTP response. */
export function useSubmission(onSaved: () => void) {
  const active = useRef<AbortController | null>(null)
  const reference = useRef<{ fingerprint: string; key: string } | null>(null)
  const [busy, setBusy] = useState(false)
  const [message, setMessage] = useState('')
  useEffect(
    () => () => {
      active.current?.abort()
    },
    []
  )
  async function submit(
    payload: unknown,
    write: (key: string, signal: AbortSignal) => Promise<unknown>
  ) {
    if (active.current) return
    const fingerprint = JSON.stringify(payload)
    if (reference.current?.fingerprint !== fingerprint)
      reference.current = { fingerprint, key: crypto.randomUUID() }
    const controller = new AbortController()
    active.current = controller
    setBusy(true)
    setMessage('')
    try {
      await write(reference.current.key, controller.signal)
      if (!controller.signal.aborted) {
        reference.current = null
        onSaved()
      }
    } catch (error) {
      if (!controller.signal.aborted) setMessage(deliveryGuidance(error))
    } finally {
      if (!controller.signal.aborted) {
        active.current = null
        setBusy(false)
      }
    }
  }
  return { busy, message, submit }
}
