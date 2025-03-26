import { writeFile, mkdir } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { $ } from 'zx'

const __filename = fileURLToPath(import.meta.url)
const __dirname = path.dirname(__filename)
const distPath = path.resolve(__dirname, '../dist')

// Create dist directory if it doesn't exist
await mkdir(distPath, { recursive: true })

const branch = (await $`git rev-parse --abbrev-ref HEAD`.text()).trim()
const version = (await $`git log -1 --pretty=format:"%h"`.text()).trim()
const date = (
  await $`TZ=UTC git log -1 --date=iso-local --pretty=format:"%ad"`.text()
).trim()

const json = JSON.stringify({ branch, version, date }, null, 2)

console.log(`Writing version.json to ${distPath}`)

await writeFile(path.resolve(distPath, 'version.json'), json)
