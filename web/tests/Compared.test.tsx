import { describe, expect, it } from 'vitest'
import { Compared } from '../src/Compared'
import { fitModule } from '../src/modules/fit'
import { mount } from './support/mount'

describe('Compared', () => {
  const cells = (reading: 'ltr' | 'rtl') => {
    const host = mount(
      <Compared
        reading={reading}
        fit={{ ...fitModule.defaults, gap: 8 }}
        companion={<i>other</i>}
      >
        <b>read</b>
      </Compared>,
    )
    return [...(host.querySelector('.compared')?.children ?? [])].map(
      (cell) => [
        cell.textContent,
        (cell as HTMLElement).style.paddingRight,
        (cell as HTMLElement).style.paddingLeft,
      ],
    )
  }

  it('sets the page read first in reading order, the two meeting at the seam as a spread’s pages do', () => {
    expect(cells('ltr')).toEqual([
      ['read', '4px', ''],
      ['other', '', '4px'],
    ])
    expect(cells('rtl')).toEqual([
      ['other', '4px', ''],
      ['read', '', '4px'],
    ])
  })
})
