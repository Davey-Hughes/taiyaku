/**
 * The characters a look-up's words name, a card each, drawn above the
 * dictionary's entries: who they are, where the books read so far first met
 * them and last saw them, and what those books say of them with each other
 * character. A fact is a row that opens on what bears it out: its quote,
 * where the books say it, and where they took it back.
 */
import { useState } from 'react'
import type { ChapterAt, CharacterCard, CharacterFact } from './book'

/**
 * The book's title, the chapter's number and its title. The number is left
 * out where the title already carries it ("Chapter 1 - …", "35장 …"), and
 * `chapterTitle` is "" for a chapter haiyaku's log lacks, which is then
 * named by its number alone.
 */
const chapterOf = (at: ChapterAt) => {
  const numbered = new RegExp(`(^|\\D)${at.chapter}(\\D|$)`).test(
    at.chapterTitle,
  )
  return [at.book, numbered ? '' : `chapter ${at.chapter}`, at.chapterTitle]
    .filter(Boolean)
    .join(', ')
}

/** What the "overturned" mark means, for a pointer resting on it. */
const OVERTURNED = 'The story later showed this to be wrong'

const sameChapter = (a: ChapterAt, b: ChapterAt) =>
  a.book === b.book && a.chapter === b.chapter

/** Each of `cards` in the answer's order. */
export function BookCharacters({ cards }: { cards: CharacterCard[] }) {
  // Nothing where no one is named, so the panel is the dictionary's alone.
  if (cards.length === 0) return null
  return (
    <div className="book-lookup-characters">
      {cards.map((card, i) => (
        <Card key={i} card={card} />
      ))}
    </div>
  )
}

function Card({ card }: { card: CharacterCard }) {
  return (
    <section className="book-lookup-character">
      <h3>{card.name.text}</h3>
      <dl className="book-lookup-about">
        {card.aliases.length > 0 && (
          <>
            <dt>Also called</dt>
            <dd className="book-lookup-aliases">
              {card.aliases.map((alias) => alias.text).join(', ')}
            </dd>
          </>
        )}
        <dt>First met</dt>
        <dd data-met="first">{chapterOf(card.first)}</dd>
        <dt>Last seen</dt>
        <dd data-met="last">{chapterOf(card.last)}</dd>
      </dl>
      {card.relations.map((relation, i) => (
        <section key={i} className="book-lookup-relation">
          <h4>{relation.with.text}</h4>
          <ul>
            {relation.facts.map((fact, j) => (
              <Fact key={j} fact={fact} />
            ))}
          </ul>
        </section>
      ))}
    </section>
  )
}

/**
 * A fact as a row to press: closed it is the fact alone, marked where the
 * books took it back; open it adds the quote, where the books say it, and
 * the overturn's own note, quote and chapter.
 */
function Fact({ fact }: { fact: CharacterFact }) {
  const [open, setOpen] = useState(false)
  return (
    <li
      className="book-lookup-fact"
      data-kind={fact.kind}
      data-overturned={fact.overturned ? '' : undefined}
    >
      <button
        type="button"
        className="book-lookup-fact-row"
        aria-expanded={open}
        onClick={() => setOpen((was) => !was)}
      >
        <span className="book-lookup-fact-note">{fact.note}</span>
        {fact.overturned && (
          <span className="book-lookup-fact-tag" title={OVERTURNED}>
            overturned
          </span>
        )}
      </button>
      {open && (
        <div className="book-lookup-more">
          <blockquote className="book-lookup-quote">
            <p>{fact.quote}</p>
          </blockquote>
          {sameChapter(fact.first, fact.last) ? (
            <p className="book-lookup-where">{chapterOf(fact.first)}</p>
          ) : (
            <>
              <p className="book-lookup-where">
                First: {chapterOf(fact.first)}
              </p>
              <p className="book-lookup-where">Last: {chapterOf(fact.last)}</p>
            </>
          )}
          {fact.overturned && (
            <>
              <p className="book-lookup-overturn">
                <span title={OVERTURNED}>Overturned:</span>{' '}
                {fact.overturned.note}
              </p>
              <blockquote className="book-lookup-quote">
                <p>{fact.overturned.quote}</p>
              </blockquote>
              <p className="book-lookup-where">
                {chapterOf(fact.overturned.at)}
              </p>
            </>
          )}
        </div>
      )}
    </li>
  )
}
