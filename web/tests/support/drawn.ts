import { page } from 'vitest/browser'
import type { Channels } from './colour'

/**
 * Reading back what the browser drew, for the cases that ask a colour of a
 * pixel rather than of a computed style: what a shape's corner, an edge's
 * contrast or a dot's size comes to only once it is painted.
 */

/** `el` as drawn: the pixels of its screenshot, read back through a canvas. */
export const drawn = async (el: Element) => {
  const png = await page.screenshot({ element: el, save: false })
  const image = new Image()
  image.src = `data:image/png;base64,${png}`
  await image.decode()
  const canvas = document.createElement('canvas')
  canvas.width = image.naturalWidth
  canvas.height = image.naturalHeight
  const context = canvas.getContext('2d')
  if (context === null) throw new Error('no canvas to read the pixels from')
  context.drawImage(image, 0, 0)
  return context.getImageData(0, 0, canvas.width, canvas.height)
}

export const pixelAt = (image: ImageData, x: number, y: number): Channels => {
  const at = (y * image.width + x) * 4
  const [r, g, b] = image.data.slice(at, at + 3)
  return { r, g, b, a: 1 }
}

/** How far apart two colours are: their channels' differences, summed. */
export const apart = (one: Channels, other: Channels) =>
  Math.abs(one.r - other.r) +
  Math.abs(one.g - other.g) +
  Math.abs(one.b - other.b)
