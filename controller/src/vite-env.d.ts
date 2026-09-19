/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** Origin of the live MeshCentral instance. Unset = mock engine. */
  readonly VITE_MESH_ORIGIN?: string
}
interface ImportMeta {
  readonly env: ImportMetaEnv
}
