/**
 * The settings pages' shared frame: a header, then sections, each a heading
 * and a bordered box of rows, the way a person scans a settings page — the
 * section by its heading, the setting by its label, the control at the
 * row's end. `styles/settings.css` draws it.
 *
 * **Every heading's size is its class's, never its tag's.** A page sits at
 * h2 under Configuration's hidden h1 and could sit at h1 on a page of its
 * own; a section is h3 beneath either. The level is the document outline
 * and the class is the look, so moving a page does not restyle it.
 */
import { type ReactElement, type ReactNode, type Ref, useId } from 'react'
import { Link, useLocation } from 'react-router'

type Level = 1 | 2 | 3 | 4

/**
 * A page's way up, drawn as `p.back` is on every page beneath a table. It
 * carries `closes` as `closed` in the router state, so the list it goes to
 * (`SideBySide`) can give focus to this page's row.
 */
export interface BackTo {
  to: string
  label: string
  /**
   * This page's address as its row's link spells it, the one `SideBySide`
   * finds the row by. The address in the bar where it is left out, which a
   * typed address may spell otherwise: in another case, or with a character
   * the link escapes left bare.
   */
  closes?: string
}

export interface PageHeaderProps {
  back?: BackTo
  title: ReactNode
  level?: Level
  /** One line under the title. A `div`, so it may hold a `dl`. */
  description?: ReactNode
  /** The page's own buttons, at the title's end where the width allows. */
  actions?: ReactNode
  /**
   * Given where the title takes focus: a page reached by an action whose
   * button went with the page it was on.
   */
  titleRef?: Ref<HTMLHeadingElement>
  /**
   * Laid over the title while the title is edited where it stands. The
   * title stays beneath it, unseen and inert, so the header keeps the size
   * the title gives it whatever is typed, and the editor takes no part in
   * the layout: an editor wearing `page-title-editor` is laid over the
   * title's first line, and a text box in it wearing `page-title-box` takes
   * the title's face and runs to the end of its column (`settings.css`).
   */
  titleEditor?: ReactElement
  /**
   * The actions while `titleEditor` is drawn, in place of `actions`. Both
   * sets are always drawn, in one slot as wide as the wider of the two, and
   * the set not in use is unseen and inert, so swapping them moves nothing.
   */
  editorActions?: ReactNode
  /**
   * What answers the edit, drawn while `titleEditor` is: why the name typed
   * would be refused, or a refusal. It is
   * laid over what is under the header, as a form's bubble is laid over the
   * form, and no room is kept for it, so that neither opening the editor nor
   * an answer coming or going moves anything on the page.
   */
  editorAnswer?: ReactNode
}

export function PageHeader({
  back,
  title,
  level = 2,
  description,
  actions,
  titleRef,
  titleEditor,
  editorActions,
  editorAnswer,
}: PageHeaderProps) {
  const Tag = `h${level}` as 'h2'
  const editing = titleEditor !== undefined
  return (
    <div className="page-header">
      {back !== undefined && <BackLink {...back} />}
      <div className="page-header-main">
        <div className="page-header-text">
          <Tag
            className="page-title"
            ref={titleRef}
            tabIndex={titleRef === undefined ? undefined : -1}
            inert={editing}
          >
            {title}
          </Tag>
          {titleEditor}
          {description !== undefined && (
            <div className="page-description">{description}</div>
          )}
        </div>
        {actions !== undefined &&
          (editorActions === undefined ? (
            <div className="page-actions">{actions}</div>
          ) : (
            <div className="page-actions page-actions-swap">
              <div className="page-actions-set" inert={editing}>
                {actions}
              </div>
              <div className="page-actions-set" inert={!editing}>
                {editorActions}
              </div>
            </div>
          ))}
      </div>
      {editing && editorAnswer !== undefined && (
        <div className="page-header-answer">{editorAnswer}</div>
      )}
    </div>
  )
}

/**
 * The body's place while a page waits; draws nothing until `drawn`, which a
 * page takes from `usePage`'s `loadingDrawn` (Licence, which reads nothing,
 * from its own `useDrawnWait`), so a wait answered within
 * `WAIT_DRAWN_AFTER_MS` draws no line at all.
 */
export function PageLoading({ drawn }: { drawn: boolean }) {
  return drawn ? <p role="status">Loading…</p> : null
}

/**
 * A component of its own so that only a page with a way up reads the
 * router's location: a page without one is drawn outside a router too.
 */
function BackLink({ to, label, closes }: BackTo) {
  const { pathname } = useLocation()
  return (
    <p className="back">
      <Link to={to} state={{ closed: closes ?? pathname }}>
        ‹ {label}
      </Link>
    </p>
  )
}

export type SectionProps = {
  level?: Level
  children: ReactNode
} & (
  | {
      /**
       * Never `undefined`, which would drop the description unseen, nor
       * `null`, which would draw an empty heading.
       */
      title: NonNullable<ReactNode>
      /** One line under the heading, outside the box. */
      description?: ReactNode
      /**
       * The heading's `id`, given where focus is sent to the heading: by a
       * fragment link that leads to the section, or by a screen that finds
       * it by this id, as a Revoke with no row left to take focus does
       * (`useFocusAfterRevoke`). The heading then has `tabIndex` -1, so it
       * takes that focus and is not a stop for Tab.
       */
      titleId?: string
      /**
       * Drawn at the end of the heading's line, outside the heading, so
       * the heading keeps its own words; under it where the line is too
       * short for both.
       */
      actions?: ReactNode
    }
  | {
      /**
       * Left out for a box the page's own title names: a page's one box,
       * or each of the panes a page sets side by side, as Rules sets its
       * list and its rule.
       */
      title?: never
      /**
       * Not taken: an untitled box has no head for it to sit in, so it
       * would be dropped unseen. The page's header holds that line.
       */
      description?: never
      /** Not taken: an untitled box has no heading for it to name. */
      titleId?: never
      /** Not taken: an untitled box has no heading line for them. */
      actions?: never
    }
)

/** A heading and its line outside, and a bordered box of rows beneath. */
export function Section(props: SectionProps) {
  return <Frame {...props} removal={false} />
}

/**
 * A page's last box, the one holding its Remove… and, once that is pressed,
 * the confirm it opens. Drawn with the danger strip the confirm boxes
 * already carry, so the one irreversible thing on the page is told apart
 * before anything is pressed.
 */
export function RemovalSection(props: SectionProps) {
  return <Frame {...props} removal />
}

function Frame({
  title,
  level = 3,
  description,
  titleId,
  actions,
  children,
  removal,
}: SectionProps & { removal: boolean }) {
  const Tag = `h${level}` as 'h3'
  return (
    <section
      className={
        removal
          ? 'settings-section settings-removal'
          : title === undefined
            ? 'settings-section settings-untitled'
            : 'settings-section'
      }
    >
      {title !== undefined && (
        <div className="settings-section-head">
          <Tag
            className="settings-section-title"
            id={titleId}
            tabIndex={titleId === undefined ? undefined : -1}
          >
            {title}
          </Tag>
          {actions !== undefined && (
            <div className="settings-section-actions">{actions}</div>
          )}
          {description !== undefined && (
            <p className="settings-section-description">{description}</p>
          )}
        </div>
      )}
      <div className="settings-box">{children}</div>
    </section>
  )
}

/**
 * The ids a form row's control wears: `control` for the `<label>`'s
 * `htmlFor`, and `help` for its `aria-describedby`, `undefined` where the
 * row has no help to point at.
 */
export interface RowIds {
  control: string
  help: string | undefined
}

export type RowProps = {
  label: ReactNode
  /** A sentence or two under the label, saying what the setting does. */
  help?: ReactNode
  /**
   * A checkbox row. Its box stays beside the label at every width, since a
   * box alone on a line under its words reads as a different control.
   */
  check?: boolean
} & (
  | {
      group?: never
      /**
       * The control. A function for a form control, which is handed the ids
       * that tie the label and the help to it, so a click on the label
       * reaches the control and the name stays the label's words alone, not
       * the help's as well. Anything else for a row that only reports: a
       * session or a key, with its Revoke at the end.
       */
      children?: ReactNode | ((ids: RowIds) => ReactNode)
    }
  | {
      /**
       * A row whose control is a choice among radios, each with a label of
       * its own. No one radio can take the row's `<label>`, so the control's
       * box is the radio group: the row's label names it and the help
       * describes it, and both are read as focus enters it. A literal role,
       * so that a group of checkboxes can be added as `'group'` beside it.
       */
      group: 'radiogroup'
      children: ReactNode
    }
)

export function Row({ label, help, check = false, group, children }: RowProps) {
  const id = useId()
  const ids: RowIds = {
    control: `${id}-control`,
    help: help === undefined ? undefined : `${id}-help`,
  }
  const labelId = group === undefined ? undefined : `${id}-label`
  const form = typeof children === 'function'
  const control = form ? children(ids) : children
  return (
    <div className={check ? 'settings-row settings-row-check' : 'settings-row'}>
      <div className="settings-row-text">
        {form ? (
          <label className="settings-row-label" htmlFor={ids.control}>
            {/* One element, because under a coarse pointer a check row's
                label is a grid (`settings.css`) and each child of a grid is
                an item of its own: words split across items lose the space
                where they join. */}
            {check ? <span>{label}</span> : label}
          </label>
        ) : (
          <div className="settings-row-label" id={labelId}>
            {label}
          </div>
        )}
        {help !== undefined && (
          <p className="settings-row-help" id={ids.help}>
            {help}
          </p>
        )}
      </div>
      {children !== undefined &&
        (group === undefined ? (
          <div className="settings-row-control">{control}</div>
        ) : (
          <div
            className="settings-row-control"
            role="radiogroup"
            aria-labelledby={labelId}
            aria-describedby={ids.help}
          >
            {control}
          </div>
        ))}
    </div>
  )
}
