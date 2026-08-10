import { describePosition, type Pending } from './correction'
import type { Position } from './resolver'

export interface CorrectionBannerProps {
  /** The assertion being made. `b` absent is "no counterpart picked yet". */
  pending: Pending
  /**
   * Which slot loses its counterpart if this lands — `correction.ts`'s
   * `displacedBy`, computed by the caller. Named before the write, never
   * after it: nothing undoes a correction short of making another.
   */
  displaced: Position | undefined
  /** A refusal or a stale-validator notice, shown beside the controls rather than logged. */
  notice: string | undefined
  /** A write is on the wire. Both controls stand down for the duration. */
  submitting: boolean
  onConfirm: () => void
  onCancel: () => void
}

/**
 * The one screen a reader confirms an irreversible write from.
 *
 * Two states, and the second is the whole reason this is a component worth
 * testing rather than eighty lines of JSX inside `App.tsx`: before a
 * counterpart is picked it is a prompt, and after one is picked it has to say
 * *what is being asserted* and *what that costs* on the same screen, with two
 * buttons whose `disabled` is the only thing standing between a reader and a
 * second concurrent write against one validator.
 *
 * `role="status"` rather than `alert`: this appears because the reader pressed
 * `c`, so it is not an interruption, and an assertive live region would make a
 * screen reader talk over the strip the reader is about to pick from.
 */
export function CorrectionBanner({
  pending,
  displaced,
  notice,
  submitting,
  onConfirm,
  onCancel,
}: CorrectionBannerProps) {
  return (
    <div
      role="status"
      style={{
        position: 'absolute',
        top: 16,
        left: 16,
        right: 16,
        zIndex: 1,
        display: 'flex',
        flexDirection: 'column',
        alignItems: 'center',
        gap: 8,
        padding: '10px 16px',
        borderRadius: 8,
        background: 'rgba(24, 24, 24, 0.92)',
        color: 'rgba(255, 255, 255, 0.92)',
        textAlign: 'center',
      }}
    >
      {pending.b === undefined ? (
        <p style={{ margin: 0 }}>
          Pick the <strong>{pending.to}</strong> page that matches{' '}
          <strong>{pending.from}</strong> {describePosition(pending.a)}.{' '}
          <em>Escape</em> cancels without writing anything.
        </p>
      ) : (
        <>
          {/* The assertion itself, restated at the moment of confirming.
              Without this the banner swapped the "pick a page" prompt for
              the consequence line alone, so the one screen a reader
              confirms an irreversible write from never said what was
              being asserted — found by driving this against the real
              corpus, where the strip has 235 tiles and the picked one
              scrolls out of view as easily as not. */}
          <p style={{ margin: 0 }}>
            Pair <strong>{pending.from}</strong> {describePosition(pending.a)}{' '}
            with <strong>{pending.to}</strong> {describePosition(pending.b)}?
          </p>
          {/* Named before the write, not after it — see `displacedBy`'s own
              doc. Shown even when `displaced` is undefined, so a reader sees
              the same confirmation shape either way rather than wondering
              whether the absence of a warning is itself the warning. */}
          <p style={{ margin: 0 }}>
            {displaced
              ? `${pending.from} ${describePosition(displaced)} will lose its ${pending.to} counterpart.`
              : `Nothing loses its counterpart — ${pending.to} ${describePosition(pending.b)} pairs with nothing today.`}
          </p>
          {notice && <p style={{ margin: 0, opacity: 0.85 }}>{notice}</p>}
          {/* Both disabled while the write is on the wire — see
              `submitting`'s doc in `App.tsx`. Cancel especially: it is the
              control that most looks like it undoes the write, and it is the
              one that cannot. */}
          <div style={{ display: 'flex', gap: 8 }}>
            <button type="button" disabled={submitting} onClick={onConfirm}>
              {submitting ? 'Writing…' : 'Confirm'}
            </button>
            <button type="button" disabled={submitting} onClick={onCancel}>
              Cancel
            </button>
          </div>
        </>
      )}
    </div>
  )
}
