import { useCallback, useEffect, useMemo, useRef } from 'react'
import { Bot, Power, RefreshCw } from 'lucide-react'
import * as THREE from 'three'
import {
  agentRuntimeLabel as agentRuntimeDisplayLabel,
  useAgentsStore,
  type AgentInfo,
  type AgentStatus,
} from '@app/entities/agent'

type AgentSceneObject = {
  group: THREE.Group
  body: THREE.Mesh<THREE.CylinderGeometry, THREE.MeshStandardMaterial>
  head: THREE.Group
  leftArm: THREE.Group
  rightArm: THREE.Group
  leftEye: THREE.Mesh<THREE.PlaneGeometry, THREE.MeshBasicMaterial>
  rightEye: THREE.Mesh<THREE.PlaneGeometry, THREE.MeshBasicMaterial>
  chest: THREE.Mesh<THREE.BoxGeometry, THREE.MeshBasicMaterial>
  statusLight: THREE.Mesh<THREE.SphereGeometry, THREE.MeshBasicMaterial>
  accentMaterials: THREE.MeshBasicMaterial[]
  beacon: THREE.Mesh<THREE.TorusGeometry, THREE.MeshBasicMaterial>
  halo: THREE.Mesh<THREE.TorusGeometry, THREE.MeshBasicMaterial>
  screenGlow: THREE.Mesh<THREE.BoxGeometry, THREE.MeshBasicMaterial>
  gearLarge: THREE.Mesh<THREE.TorusGeometry, THREE.MeshStandardMaterial>
  gearSmall: THREE.Mesh<THREE.TorusGeometry, THREE.MeshStandardMaterial>
  hammer: THREE.Group
  dataBlocks: THREE.Mesh<THREE.BoxGeometry, THREE.MeshBasicMaterial>[]
  taskCards: THREE.Mesh<THREE.BoxGeometry, THREE.MeshStandardMaterial>[]
}

type SceneRuntime = {
  scene: THREE.Scene
  camera: THREE.PerspectiveCamera
  renderer: THREE.WebGLRenderer
  raycaster: THREE.Raycaster
  pointer: THREE.Vector2
  agentObjects: Map<string, AgentSceneObject>
  frameId: number
  resizeObserver: ResizeObserver
  handlePointerDown: (event: PointerEvent) => void
}

const STATUS_STYLE: Record<
  AgentStatus,
  { label: string; color: number; emissive: number; className: string; desk: number }
> = {
  working: {
    label: 'Working',
    color: 0x34c759,
    emissive: 0x103c20,
    className: 'bg-emerald-500',
    desk: 0xffb340,
  },
  idle: {
    label: 'Ready',
    color: 0x0a84ff,
    emissive: 0x06284d,
    className: 'bg-sky-500',
    desk: 0x4ac8e8,
  },
  offline: {
    label: 'Offline',
    color: 0x8e8e93,
    emissive: 0x1d1d21,
    className: 'bg-zinc-500',
    desk: 0x6f737d,
  },
}

const EMPTY_STATE_STEPS = [
  { label: 'Open Agents and create one if none exists', icon: Bot },
  { label: 'Start or wake the agent if it is already listed', icon: Power },
  { label: 'Refresh this view after the agent checks in', icon: RefreshCw },
]

export function workshop3DAgentSubtitle(agent: AgentInfo): string {
  return `${STATUS_STYLE[agent.status].label} - ${agentRuntimeDisplayLabel(agent)}`
}

export function Workshop3DEmptyState() {
  return (
    <div
      data-testid="workshop-3d-empty-state"
      className="space-y-3 px-2 py-1 text-xs leading-5 text-white/70"
    >
      <div>
        <p className="text-sm font-medium leading-5 text-white">No agents in the workshop yet</p>
        <p className="mt-1">
          If this is your first agent, create it from Agents. If you already have one, start or wake
          it there, then refresh this view.
        </p>
      </div>
      <ol className="space-y-2">
        {EMPTY_STATE_STEPS.map(({ label, icon: Icon }) => (
          <li key={label} className="flex items-start gap-2">
            <span className="mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-white/10 text-white/80">
              <Icon className="h-3.5 w-3.5" aria-hidden="true" />
            </span>
            <span className="min-w-0">{label}</span>
          </li>
        ))}
      </ol>
    </div>
  )
}

function agentPosition(index: number, total: number): THREE.Vector3 {
  if (total <= 1) return new THREE.Vector3(0, 0, 0.55)
  const radius = Math.min(4.9, Math.max(2.7, 1.8 + total * 0.48))
  const angle = (index / total) * Math.PI * 2 - Math.PI / 2
  return new THREE.Vector3(Math.cos(angle) * radius, 0, Math.sin(angle) * radius)
}

function createArm(side: -1 | 1, material: THREE.MeshStandardMaterial): THREE.Group {
  const arm = new THREE.Group()
  arm.position.set(side * 0.29, 0.58, 0.04)

  const shoulder = new THREE.Mesh(new THREE.SphereGeometry(0.055, 16, 10), material)
  shoulder.castShadow = true
  arm.add(shoulder)

  const upperArm = new THREE.Mesh(new THREE.CapsuleGeometry(0.042, 0.24, 4, 10), material)
  upperArm.position.y = -0.16
  upperArm.castShadow = true
  arm.add(upperArm)

  const hand = new THREE.Mesh(
    new THREE.SphereGeometry(0.052, 16, 10),
    new THREE.MeshStandardMaterial({
      color: 0x9aa8b8,
      metalness: 0.55,
      roughness: 0.34,
    })
  )
  hand.position.y = -0.32
  hand.castShadow = true
  arm.add(hand)

  arm.rotation.z = side * 0.28
  arm.rotation.x = -0.62
  return arm
}

function createRobotHead(
  metalMaterial: THREE.MeshStandardMaterial,
  statusColor: number
): Pick<AgentSceneObject, 'head' | 'leftEye' | 'rightEye' | 'statusLight' | 'accentMaterials'> {
  const head = new THREE.Group()
  head.position.y = 0.9
  const accentMaterials: THREE.MeshBasicMaterial[] = []

  const shellGeometry = new THREE.SphereGeometry(0.27, 32, 24)
  shellGeometry.scale(1.08, 0.9, 0.84)
  const shell = new THREE.Mesh(shellGeometry, metalMaterial)
  shell.castShadow = true
  head.add(shell)

  const visorMaterial = new THREE.MeshBasicMaterial({
    color: 0x0c1220,
    transparent: true,
    opacity: 0.94,
  })
  const visor = new THREE.Mesh(new THREE.BoxGeometry(0.39, 0.19, 0.018), visorMaterial)
  visor.position.set(0, 0.025, 0.228)
  head.add(visor)

  const eyeMaterial = new THREE.MeshBasicMaterial({
    color: statusColor,
    transparent: true,
    opacity: 0.95,
  })
  const leftEye = new THREE.Mesh(new THREE.PlaneGeometry(0.052, 0.074), eyeMaterial.clone())
  leftEye.position.set(-0.085, 0.045, 0.239)
  accentMaterials.push(leftEye.material)
  head.add(leftEye)

  const rightEye = new THREE.Mesh(new THREE.PlaneGeometry(0.052, 0.074), eyeMaterial.clone())
  rightEye.position.set(0.085, 0.045, 0.239)
  accentMaterials.push(rightEye.material)
  head.add(rightEye)

  const mouthMaterial = new THREE.MeshBasicMaterial({
    color: statusColor,
    transparent: true,
    opacity: 0.75,
  })
  const mouth = new THREE.Mesh(new THREE.BoxGeometry(0.11, 0.012, 0.006), mouthMaterial)
  mouth.position.set(0, -0.045, 0.242)
  accentMaterials.push(mouthMaterial)
  head.add(mouth)

  const rearVisor = new THREE.Mesh(new THREE.BoxGeometry(0.36, 0.16, 0.018), visorMaterial.clone())
  rearVisor.position.set(0, 0.02, -0.228)
  rearVisor.rotation.y = Math.PI
  head.add(rearVisor)

  for (const x of [-0.07, 0.07]) {
    const rearEye = new THREE.Mesh(new THREE.PlaneGeometry(0.046, 0.058), eyeMaterial.clone())
    rearEye.position.set(x, 0.035, -0.239)
    rearEye.rotation.y = Math.PI
    accentMaterials.push(rearEye.material)
    head.add(rearEye)
  }

  const rearMouth = new THREE.Mesh(new THREE.BoxGeometry(0.09, 0.01, 0.006), mouthMaterial.clone())
  rearMouth.position.set(0, -0.04, -0.242)
  rearMouth.rotation.y = Math.PI
  accentMaterials.push(rearMouth.material)
  head.add(rearMouth)

  const earMaterial = new THREE.MeshStandardMaterial({
    color: 0x475569,
    metalness: 0.62,
    roughness: 0.3,
  })
  const earGeometry = new THREE.CylinderGeometry(0.056, 0.056, 0.04, 20)
  const leftEar = new THREE.Mesh(earGeometry, earMaterial)
  leftEar.rotation.z = Math.PI / 2
  leftEar.position.set(-0.27, 0.02, 0)
  head.add(leftEar)

  const rightEar = new THREE.Mesh(earGeometry, earMaterial.clone())
  rightEar.rotation.z = Math.PI / 2
  rightEar.position.set(0.27, 0.02, 0)
  head.add(rightEar)

  const antenna = new THREE.Group()
  antenna.position.y = 0.26
  const mast = new THREE.Mesh(
    new THREE.CylinderGeometry(0.012, 0.016, 0.19, 10),
    new THREE.MeshStandardMaterial({ color: 0x91a0b2, metalness: 0.72, roughness: 0.22 })
  )
  mast.position.y = 0.09
  antenna.add(mast)
  const statusLight = new THREE.Mesh(
    new THREE.SphereGeometry(0.045, 16, 12),
    new THREE.MeshBasicMaterial({ color: statusColor, transparent: true, opacity: 0.96 })
  )
  statusLight.position.y = 0.2
  accentMaterials.push(statusLight.material)
  antenna.add(statusLight)
  head.add(antenna)

  return { head, leftEye, rightEye, statusLight, accentMaterials }
}

function createWorkPod(
  statusColor: number
): Pick<
  AgentSceneObject,
  'screenGlow' | 'gearLarge' | 'gearSmall' | 'hammer' | 'dataBlocks' | 'taskCards'
> & { group: THREE.Group } {
  const group = new THREE.Group()
  group.position.set(0, 0, 0.66)

  const tableMaterial = new THREE.MeshStandardMaterial({
    color: 0x20242d,
    metalness: 0.32,
    roughness: 0.58,
  })
  const table = new THREE.Mesh(new THREE.BoxGeometry(1.16, 0.08, 0.58), tableMaterial)
  table.position.y = 0.33
  table.castShadow = true
  table.receiveShadow = true
  group.add(table)

  const legMaterial = new THREE.MeshStandardMaterial({
    color: 0x11151d,
    metalness: 0.45,
    roughness: 0.42,
  })
  for (const x of [-0.48, 0.48]) {
    for (const z of [-0.21, 0.21]) {
      const leg = new THREE.Mesh(new THREE.CylinderGeometry(0.025, 0.03, 0.54, 8), legMaterial)
      leg.position.set(x, 0.07, z)
      leg.castShadow = true
      group.add(leg)
    }
  }

  const screenGlow = new THREE.Mesh(
    new THREE.BoxGeometry(0.54, 0.32, 0.028),
    new THREE.MeshBasicMaterial({
      color: statusColor,
      transparent: true,
      opacity: 0.42,
    })
  )
  screenGlow.position.set(0.27, 0.58, -0.18)
  screenGlow.rotation.x = -0.42

  const screenFrame = new THREE.Mesh(
    new THREE.BoxGeometry(0.61, 0.39, 0.035),
    new THREE.MeshStandardMaterial({
      color: 0x0f1724,
      metalness: 0.55,
      roughness: 0.32,
    })
  )
  screenFrame.position.copy(screenGlow.position)
  screenFrame.rotation.copy(screenGlow.rotation)
  screenFrame.position.z -= 0.018
  group.add(screenFrame)
  group.add(screenGlow)

  const lineMaterial = new THREE.MeshBasicMaterial({
    color: 0xd9f7ff,
    transparent: true,
    opacity: 0.78,
  })
  for (let i = 0; i < 3; i += 1) {
    const line = new THREE.Mesh(new THREE.BoxGeometry(0.32 - i * 0.05, 0.012, 0.006), lineMaterial)
    line.position.set(0.27, 0.64 - i * 0.065, -0.155)
    line.rotation.copy(screenGlow.rotation)
    group.add(line)
  }

  const board = new THREE.Mesh(
    new THREE.BoxGeometry(0.64, 0.48, 0.035),
    new THREE.MeshStandardMaterial({
      color: 0x343545,
      metalness: 0.05,
      roughness: 0.8,
    })
  )
  board.position.set(-0.36, 0.72, -0.25)
  group.add(board)

  const taskCards: THREE.Mesh<THREE.BoxGeometry, THREE.MeshStandardMaterial>[] = []
  const cardColors = [0x4ade80, 0xfbbf24, 0x60a5fa, 0xf472b6]
  const cardPositions = [
    [-0.5, 0.82, -0.226],
    [-0.26, 0.82, -0.226],
    [-0.5, 0.65, -0.226],
    [-0.26, 0.65, -0.226],
  ] as const
  cardPositions.forEach((position, index) => {
    const card = new THREE.Mesh(
      new THREE.BoxGeometry(0.16, 0.105, 0.01),
      new THREE.MeshStandardMaterial({
        color: cardColors[index],
        roughness: 0.78,
      })
    )
    card.position.set(position[0], position[1], position[2])
    taskCards.push(card)
    group.add(card)
  })

  const gearMaterial = new THREE.MeshStandardMaterial({
    color: 0xff9f0a,
    metalness: 0.72,
    roughness: 0.28,
  })
  const gearLarge = new THREE.Mesh(new THREE.TorusGeometry(0.11, 0.024, 8, 16), gearMaterial)
  gearLarge.position.set(-0.16, 0.39, 0.08)
  gearLarge.rotation.x = Math.PI / 2
  group.add(gearLarge)

  const gearSmall = new THREE.Mesh(
    new THREE.TorusGeometry(0.075, 0.018, 8, 14),
    gearMaterial.clone()
  )
  gearSmall.position.set(-0.31, 0.39, 0.13)
  gearSmall.rotation.x = Math.PI / 2
  group.add(gearSmall)

  const hammer = new THREE.Group()
  hammer.position.set(-0.03, 0.45, 0.15)
  const hammerHandle = new THREE.Mesh(
    new THREE.CylinderGeometry(0.017, 0.02, 0.33, 8),
    new THREE.MeshStandardMaterial({ color: 0x67758a, metalness: 0.42, roughness: 0.4 })
  )
  hammerHandle.rotation.z = Math.PI / 2 + 0.2
  hammer.add(hammerHandle)
  const hammerHead = new THREE.Mesh(
    new THREE.BoxGeometry(0.16, 0.075, 0.075),
    new THREE.MeshStandardMaterial({ color: 0xb7c2d1, metalness: 0.82, roughness: 0.18 })
  )
  hammerHead.position.set(-0.16, 0.03, 0)
  hammerHead.rotation.z = 0.2
  hammer.add(hammerHead)
  group.add(hammer)

  const dataBlocks: THREE.Mesh<THREE.BoxGeometry, THREE.MeshBasicMaterial>[] = []
  for (let i = 0; i < 4; i += 1) {
    const block = new THREE.Mesh(
      new THREE.BoxGeometry(0.065, 0.065, 0.065),
      new THREE.MeshBasicMaterial({
        color: i % 2 === 0 ? 0x4ac8e8 : 0xffb340,
        transparent: true,
        opacity: 0.68,
      })
    )
    block.position.set(0.02 + i * 0.12, 0.78 + i * 0.03, -0.04 - i * 0.035)
    dataBlocks.push(block)
    group.add(block)
  }

  return { group, screenGlow, gearLarge, gearSmall, hammer, dataBlocks, taskCards }
}

function createAgentObject(agent: AgentInfo): AgentSceneObject {
  const status = STATUS_STYLE[agent.status]
  const group = new THREE.Group()
  group.name = `agent-${agent.id}`
  group.userData.agentId = agent.id
  group.userData.status = agent.status

  const metalMaterial = new THREE.MeshStandardMaterial({
    color: 0x2a3441,
    emissive: status.emissive,
    emissiveIntensity: agent.status === 'working' ? 0.22 : 0.08,
    metalness: 0.64,
    roughness: 0.32,
  })

  const base = new THREE.Mesh(
    new THREE.CylinderGeometry(0.82, 0.94, 0.1, 36),
    new THREE.MeshStandardMaterial({
      color: 0x121821,
      metalness: 0.4,
      roughness: 0.5,
    })
  )
  base.position.y = 0.05
  base.receiveShadow = true
  group.add(base)

  const body = new THREE.Mesh(new THREE.CylinderGeometry(0.23, 0.29, 0.42, 28), metalMaterial)
  body.position.y = 0.43
  body.castShadow = true
  body.userData.agentId = agent.id
  group.add(body)

  const chest = new THREE.Mesh(
    new THREE.BoxGeometry(0.18, 0.13, 0.012),
    new THREE.MeshBasicMaterial({
      color: status.color,
      transparent: true,
      opacity: agent.status === 'offline' ? 0.32 : 0.78,
    })
  )
  chest.position.set(0, 0.47, 0.255)
  group.add(chest)

  const { head, leftEye, rightEye, statusLight, accentMaterials } = createRobotHead(
    metalMaterial,
    status.color
  )
  head.userData.agentId = agent.id
  group.add(head)

  const leftArm = createArm(-1, metalMaterial)
  const rightArm = createArm(1, metalMaterial)
  group.add(leftArm)
  group.add(rightArm)

  const workPod = createWorkPod(status.desk)
  group.add(workPod.group)

  const beacon = new THREE.Mesh(
    new THREE.TorusGeometry(0.92, 0.018, 8, 64),
    new THREE.MeshBasicMaterial({
      color: status.color,
      transparent: true,
      opacity: agent.status === 'offline' ? 0.18 : 0.6,
    })
  )
  beacon.position.y = 0.13
  beacon.rotation.x = Math.PI / 2
  group.add(beacon)

  const halo = new THREE.Mesh(
    new THREE.TorusGeometry(1.08, 0.028, 8, 72),
    new THREE.MeshBasicMaterial({
      color: 0xffffff,
      transparent: true,
      opacity: 0.82,
    })
  )
  halo.position.y = 0.54
  halo.rotation.x = Math.PI / 2
  halo.visible = false
  group.add(halo)

  group.scale.setScalar(0.92)

  return {
    group,
    body,
    head,
    leftArm,
    rightArm,
    leftEye,
    rightEye,
    chest,
    statusLight,
    accentMaterials,
    beacon,
    halo,
    screenGlow: workPod.screenGlow,
    gearLarge: workPod.gearLarge,
    gearSmall: workPod.gearSmall,
    hammer: workPod.hammer,
    dataBlocks: workPod.dataBlocks,
    taskCards: workPod.taskCards,
  }
}

function updateAgentObject(object: AgentSceneObject, agent: AgentInfo, selected: boolean): void {
  const status = STATUS_STYLE[agent.status]
  object.group.userData.status = agent.status
  object.body.material.color.setHex(agent.status === 'offline' ? 0x3a414b : 0x2a3441)
  object.body.material.emissive.setHex(status.emissive)
  object.body.material.emissiveIntensity = agent.status === 'working' ? 0.26 : 0.08
  object.chest.material.color.setHex(status.color)
  object.chest.material.opacity = agent.status === 'offline' ? 0.32 : 0.78
  object.accentMaterials.forEach((material) => material.color.setHex(status.color))
  object.beacon.material.color.setHex(status.color)
  object.beacon.material.opacity = agent.status === 'offline' ? 0.18 : 0.6
  object.screenGlow.material.color.setHex(status.desk)
  object.screenGlow.material.opacity = agent.status === 'offline' ? 0.18 : 0.42
  object.dataBlocks.forEach((block, index) => {
    block.visible = agent.status !== 'offline' && (agent.status === 'working' || index < 2)
  })
  object.taskCards.forEach((card, index) => {
    card.material.emissive.setHex(agent.status === 'working' && index < 2 ? 0x2b1b02 : 0x000000)
    card.material.emissiveIntensity = agent.status === 'working' && index < 2 ? 0.22 : 0
  })
  object.halo.visible = selected
}

function disposeObject3D(object: THREE.Object3D): void {
  const geometries = new Set<THREE.BufferGeometry>()
  const materials = new Set<THREE.Material>()

  object.traverse((child) => {
    const mesh = child as THREE.Mesh
    if (mesh.geometry) geometries.add(mesh.geometry)
    const material = mesh.material
    if (Array.isArray(material)) {
      for (const item of material) materials.add(item)
    } else if (material) {
      materials.add(material)
    }
  })

  for (const geometry of geometries) geometry.dispose()
  for (const material of materials) material.dispose()
}

function reconcileAgents(
  runtime: SceneRuntime,
  agents: AgentInfo[],
  selectedAgentId: string | null
): void {
  const liveIds = new Set(agents.map((agent) => agent.id))

  for (const [agentId, object] of runtime.agentObjects) {
    if (!liveIds.has(agentId)) {
      runtime.scene.remove(object.group)
      disposeObject3D(object.group)
      runtime.agentObjects.delete(agentId)
    }
  }

  agents.forEach((agent, index) => {
    let object = runtime.agentObjects.get(agent.id)
    if (!object) {
      object = createAgentObject(agent)
      runtime.agentObjects.set(agent.id, object)
      runtime.scene.add(object.group)
    }

    const position = agentPosition(index, agents.length)
    object.group.position.set(position.x, position.y, position.z)
    object.group.rotation.y = position.lengthSq() > 0.01 ? Math.atan2(position.x, position.z) : 0
    object.group.userData.phase = index * 0.83
    updateAgentObject(object, agent, selectedAgentId === agent.id)
  })
}

function countByStatus(agents: AgentInfo[]): Record<AgentStatus, number> {
  return agents.reduce(
    (acc, agent) => {
      acc[agent.status] += 1
      return acc
    },
    { working: 0, idle: 0, offline: 0 } as Record<AgentStatus, number>
  )
}

export function Workshop3DStatusSummary({ totals }: { totals: Record<AgentStatus, number> }) {
  return (
    <div className="mt-2 flex flex-wrap gap-x-2 gap-y-1 text-[11px] text-white/70">
      <span>{totals.working} Working</span>
      <span>{totals.idle} Ready</span>
      <span>{totals.offline} Offline</span>
    </div>
  )
}

export function Workshop3DView() {
  const containerRef = useRef<HTMLDivElement>(null)
  const runtimeRef = useRef<SceneRuntime | null>(null)
  const selectedAgentIdRef = useRef<string | null>(null)
  const selectAgentRef = useRef<(id: string | null) => void>(() => undefined)
  const agentsRequestedRef = useRef(false)

  const agents = useAgentsStore((state) => state.agents)
  const selectedAgentId = useAgentsStore((state) => state.selectedAgentId)
  const loading = useAgentsStore((state) => state.loading)
  const loadAgents = useAgentsStore((state) => state.loadAgents)
  const selectAgent = useAgentsStore((state) => state.selectAgent)

  useEffect(() => {
    selectAgentRef.current = selectAgent
  }, [selectAgent])

  useEffect(() => {
    if (agentsRequestedRef.current || agents.length > 0) return
    agentsRequestedRef.current = true
    void loadAgents()
  }, [agents.length, loadAgents])

  useEffect(() => {
    const container = containerRef.current
    if (!container) return

    const scene = new THREE.Scene()
    scene.background = new THREE.Color(0x080a0f)
    scene.fog = new THREE.Fog(0x080a0f, 8, 19)

    const camera = new THREE.PerspectiveCamera(46, 1, 0.1, 100)
    camera.position.set(0, 5.6, 8.9)
    camera.lookAt(0, 0.42, 0)

    const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: false })
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2))
    renderer.shadowMap.enabled = true
    renderer.domElement.dataset.testid = 'workshop-3d-canvas'
    renderer.domElement.className = 'block h-full w-full'
    renderer.domElement.setAttribute('aria-label', 'Robot workshop agent activity scene')
    container.appendChild(renderer.domElement)

    const ambient = new THREE.AmbientLight(0xffffff, 0.38)
    scene.add(ambient)

    const keyLight = new THREE.DirectionalLight(0xffffff, 1.45)
    keyLight.position.set(3.8, 7.2, 4.8)
    keyLight.castShadow = true
    keyLight.shadow.mapSize.set(1024, 1024)
    scene.add(keyLight)

    const fillLight = new THREE.PointLight(0x4ac8e8, 1.4, 12)
    fillLight.position.set(-4, 3.4, -3)
    scene.add(fillLight)

    const warmBenchLight = new THREE.PointLight(0xffb340, 0.9, 9)
    warmBenchLight.position.set(2.8, 2.2, -2.6)
    scene.add(warmBenchLight)

    const floor = new THREE.Mesh(
      new THREE.CircleGeometry(6.8, 96),
      new THREE.MeshStandardMaterial({
        color: 0x10131a,
        metalness: 0.22,
        roughness: 0.72,
      })
    )
    floor.rotation.x = -Math.PI / 2
    floor.receiveShadow = true
    scene.add(floor)

    const grid = new THREE.GridHelper(13.2, 24, 0x375f7b, 0x202531)
    grid.position.y = 0.012
    scene.add(grid)

    const assemblyLine = new THREE.Mesh(
      new THREE.BoxGeometry(3.8, 0.045, 0.72),
      new THREE.MeshStandardMaterial({
        color: 0x22232d,
        metalness: 0.36,
        roughness: 0.52,
      })
    )
    assemblyLine.position.set(0, 0.04, -0.06)
    assemblyLine.receiveShadow = true
    scene.add(assemblyLine)

    const cyanRail = new THREE.Mesh(
      new THREE.BoxGeometry(3.65, 0.018, 0.035),
      new THREE.MeshBasicMaterial({ color: 0x4ac8e8, transparent: true, opacity: 0.72 })
    )
    cyanRail.position.set(0, 0.077, -0.38)
    scene.add(cyanRail)

    const amberRail = new THREE.Mesh(
      new THREE.BoxGeometry(3.65, 0.018, 0.035),
      new THREE.MeshBasicMaterial({ color: 0xffb340, transparent: true, opacity: 0.68 })
    )
    amberRail.position.set(0, 0.078, 0.26)
    scene.add(amberRail)

    const orbit = new THREE.Mesh(
      new THREE.TorusGeometry(3.75, 0.012, 6, 112),
      new THREE.MeshBasicMaterial({ color: 0x586171, transparent: true, opacity: 0.54 })
    )
    orbit.rotation.x = Math.PI / 2
    orbit.position.y = 0.035
    scene.add(orbit)

    const outerOrbit = new THREE.Mesh(
      new THREE.TorusGeometry(5.28, 0.009, 6, 128),
      new THREE.MeshBasicMaterial({ color: 0xffb340, transparent: true, opacity: 0.28 })
    )
    outerOrbit.rotation.x = Math.PI / 2
    outerOrbit.position.y = 0.038
    scene.add(outerOrbit)

    const raycaster = new THREE.Raycaster()
    const pointer = new THREE.Vector2()
    const resize = () => {
      const width = Math.max(1, container.clientWidth)
      const height = Math.max(1, container.clientHeight)
      camera.aspect = width / height
      camera.updateProjectionMatrix()
      renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2))
      renderer.setSize(width, height)
    }

    const runtime: SceneRuntime = {
      scene,
      camera,
      renderer,
      raycaster,
      pointer,
      agentObjects: new Map(),
      frameId: 0,
      resizeObserver: new ResizeObserver(resize),
      handlePointerDown: (event: PointerEvent) => {
        const rect = renderer.domElement.getBoundingClientRect()
        pointer.x = ((event.clientX - rect.left) / rect.width) * 2 - 1
        pointer.y = -((event.clientY - rect.top) / rect.height) * 2 + 1
        raycaster.setFromCamera(pointer, camera)

        const targets = Array.from(runtime.agentObjects.values(), (object) => object.body)
        const [hit] = raycaster.intersectObjects(targets, false)
        const agentId = hit?.object.userData.agentId
        if (typeof agentId === 'string') selectAgentRef.current(agentId)
      },
    }

    runtime.resizeObserver.observe(container)
    resize()
    renderer.domElement.addEventListener('pointerdown', runtime.handlePointerDown)
    runtimeRef.current = runtime

    const animate = () => {
      const time = performance.now() * 0.001
      for (const [agentId, object] of runtime.agentObjects) {
        const phase = Number(object.group.userData.phase ?? 0)
        const selected = selectedAgentIdRef.current === agentId
        const working = object.group.userData.status === 'working'
        const offline = object.group.userData.status === 'offline'
        const pulse = Math.sin(time * 2.2 + phase)
        object.group.position.y = Math.sin(time * (working ? 2.2 : 1.35) + phase) * 0.035
        object.body.scale.setScalar(selected ? 1.08 : 1)
        object.head.rotation.y = working ? Math.sin(time * 3.2 + phase) * 0.12 : pulse * 0.04
        object.head.rotation.z = working ? Math.sin(time * 2.1 + phase) * 0.035 : 0

        if (offline) {
          object.leftArm.rotation.x = -0.12
          object.rightArm.rotation.x = -0.12
          object.leftArm.rotation.z = -0.18
          object.rightArm.rotation.z = 0.18
          object.leftEye.scale.y = 0.34
          object.rightEye.scale.y = 0.34
        } else if (working) {
          const typeCycle = Math.sin(time * 13 + phase)
          const hammerCycle = Math.abs(Math.sin(time * 8.6 + phase))
          object.leftArm.rotation.x = -0.78 + typeCycle * 0.08
          object.leftArm.rotation.z = -0.34
          object.rightArm.rotation.x = -0.96 - hammerCycle * 0.54
          object.rightArm.rotation.z = 0.24 + Math.sin(time * 4 + phase) * 0.08
          object.leftEye.scale.y = 1 + Math.sin(time * 8 + phase) * 0.08
          object.rightEye.scale.y = 1 + Math.sin(time * 8 + phase + 0.3) * 0.08
        } else {
          object.leftArm.rotation.x = -0.52 + pulse * 0.04
          object.rightArm.rotation.x = -0.56 - pulse * 0.035
          object.leftArm.rotation.z = -0.24
          object.rightArm.rotation.z = 0.24
          object.leftEye.scale.y = 1
          object.rightEye.scale.y = 1
        }

        object.beacon.rotation.z = time * (working ? 1.45 : 0.5) + phase
        object.halo.rotation.z = -time * 0.65
        object.halo.scale.setScalar(1 + Math.sin(time * 2.4) * 0.035)
        object.gearLarge.rotation.z = time * (working ? 2.8 : 0.45) + phase
        object.gearSmall.rotation.z = -time * (working ? 3.9 : 0.7) - phase
        object.hammer.rotation.x = working ? -Math.abs(Math.sin(time * 8.6 + phase)) * 0.55 : 0
        object.statusLight.scale.setScalar(offline ? 0.78 : 1 + Math.max(0, pulse) * 0.16)
        object.screenGlow.material.opacity = offline
          ? 0.18
          : working
            ? 0.42 + Math.max(0, Math.sin(time * 5.4 + phase)) * 0.28
            : 0.34
        object.dataBlocks.forEach((block, index) => {
          block.position.y = 0.78 + index * 0.03 + Math.sin(time * 2.6 + phase + index) * 0.035
          block.rotation.y = time * (0.6 + index * 0.18)
          block.rotation.x = time * 0.35
        })
      }
      orbit.rotation.z = time * 0.035
      outerOrbit.rotation.z = -time * 0.025
      renderer.render(scene, camera)
      runtime.frameId = requestAnimationFrame(animate)
    }
    animate()

    return () => {
      cancelAnimationFrame(runtime.frameId)
      renderer.domElement.removeEventListener('pointerdown', runtime.handlePointerDown)
      runtime.resizeObserver.disconnect()
      runtimeRef.current = null
      disposeObject3D(scene)
      scene.clear()
      renderer.dispose()
      if (renderer.domElement.parentNode === container) {
        container.removeChild(renderer.domElement)
      }
    }
  }, [])

  useEffect(() => {
    selectedAgentIdRef.current = selectedAgentId
    const runtime = runtimeRef.current
    if (runtime) reconcileAgents(runtime, agents, selectedAgentId)
  }, [agents, selectedAgentId])

  const selectedAgent = useMemo(
    () => agents.find((agent) => agent.id === selectedAgentId) ?? null,
    [agents, selectedAgentId]
  )
  const totals = useMemo(() => countByStatus(agents), [agents])

  const handleSelect = useCallback(
    (agentId: string) => {
      selectAgent(agentId)
    },
    [selectAgent]
  )

  return (
    <div
      ref={containerRef}
      data-testid="workshop-3d-scene"
      className="relative h-full min-h-[420px] w-full overflow-hidden bg-[#080a0f]"
    >
      <div className="pointer-events-none absolute left-3 top-3 z-10 max-w-[calc(100%-1.5rem)] rounded-lg border border-white/10 bg-black/35 px-3 py-2 text-white shadow-lg backdrop-blur sm:left-4 sm:top-4">
        <div className="text-[11px] font-semibold text-white/55">Robot Workshop</div>
        <div data-testid="workshop-3d-agent-count" className="mt-1 text-sm font-medium">
          {loading && agents.length === 0
            ? 'Syncing agents'
            : `${agents.length} agent${agents.length === 1 ? '' : 's'}`}
        </div>
        <Workshop3DStatusSummary totals={totals} />
      </div>

      <div className="absolute inset-x-3 bottom-3 z-10 flex max-h-36 flex-col gap-2 overflow-y-auto rounded-lg border border-white/10 bg-black/35 p-2 text-white shadow-lg backdrop-blur sm:inset-x-auto sm:bottom-auto sm:right-4 sm:top-4 sm:max-h-[calc(100%-2rem)] sm:w-64">
        {agents.length === 0 && !loading ? (
          <Workshop3DEmptyState />
        ) : (
          agents.map((agent) => {
            const status = STATUS_STYLE[agent.status]
            const selected = agent.id === selectedAgentId
            return (
              <button
                key={agent.id}
                type="button"
                aria-pressed={selected}
                data-testid="workshop-3d-agent"
                data-agent-id={agent.id}
                onClick={() => handleSelect(agent.id)}
                className={[
                  'flex min-h-12 w-full items-center gap-3 rounded-md border px-3 py-2 text-left transition-colors',
                  selected
                    ? 'border-white/40 bg-white/20'
                    : 'border-white/10 bg-white/10 hover:bg-white/20',
                ].join(' ')}
              >
                <span
                  className={`h-2.5 w-2.5 shrink-0 rounded-full ${status.className}`}
                  aria-hidden="true"
                />
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-sm font-medium">{agent.name}</span>
                  <span className="block truncate text-[11px] text-white/58">
                    {workshop3DAgentSubtitle(agent)}
                  </span>
                </span>
              </button>
            )
          })
        )}
      </div>

      {selectedAgent ? (
        <div
          data-testid="workshop-3d-selected-agent"
          className="pointer-events-none absolute bottom-4 left-4 z-10 max-w-[calc(100%-2rem)] rounded-lg border border-white/10 bg-black/35 px-3 py-2 text-white shadow-lg backdrop-blur"
        >
          <div className="text-[11px] text-white/55">Selected</div>
          <div className="mt-1 truncate text-sm font-medium">{selectedAgent.name}</div>
          <div className="mt-0.5 truncate text-[11px] text-white/60">
            {workshop3DAgentSubtitle(selectedAgent)}
          </div>
        </div>
      ) : null}
    </div>
  )
}
