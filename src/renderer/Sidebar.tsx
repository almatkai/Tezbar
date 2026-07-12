import { JSX, useState } from 'react'

export type SidebarSurface =
  | 'command'
  | 'ai-chat'
  | 'clipboard'
  | 'snippets'
  | 'notes'
  | 'emoji-picker'
  | 'open-ports'
  | 'terminal'
  | 'extensions'
  | 'settings'

interface SidebarProps {
  activeSurface: SidebarSurface
  onSelectSurface: (surface: SidebarSurface) => void
}

type SidebarItem = {
  id: SidebarSurface
  label: string
  icon: (active: boolean) => JSX.Element
}

type SidebarCategory = {
  title: string
  items: SidebarItem[]
}

export default function Sidebar({ activeSurface, onSelectSurface }: SidebarProps): JSX.Element {
  const [hoveredDot, setHoveredDot] = useState<boolean>(false)

  const handleClose = (): void => {
    void window.tezbar.closeCurrentWindow()
  }

  const handleMinimize = (): void => {
    void window.tezbar.hide()
  }

  const categories: SidebarCategory[] = [
    {
      title: 'Search & AI',
      items: [
        {
          id: 'command',
          label: 'Smart Scan',
          icon: (active) => (
            <svg
              className={`h-4 w-4 transition-colors duration-200 ${
                active ? 'text-indigo-300' : 'text-ink-3 group-hover:text-ink-1'
              }`}
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              strokeWidth={2}
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"
              />
            </svg>
          ),
        },
        {
          id: 'ai-chat',
          label: 'AI Assistant',
          icon: (active) => (
            <svg
              className={`h-4 w-4 transition-colors duration-200 ${
                active ? 'text-purple-300' : 'text-ink-3 group-hover:text-ink-1'
              }`}
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              strokeWidth={2}
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="M9.813 15.904L9 21l-1.813-5.096L2.091 14.09 7.188 13.18 8 8l1.813 5.18 5.096.91-5.096.914zM19 10l.5 2.5 2.5.5-2.5.5-.5 2.5-.5-2.5-2.5-.5 2.5-.5.5-2.5zM14 3l.3 1.5 1.5.3-1.5.3-.3 1.5-.3-1.5-1.5-.3 1.5-.3.3-1.5z"
              />
            </svg>
          ),
        },
      ],
    },
    {
      title: 'Utilities',
      items: [
        {
          id: 'clipboard',
          label: 'Clipboard',
          icon: (active) => (
            <svg
              className={`h-4 w-4 transition-colors duration-200 ${
                active ? 'text-blue-300' : 'text-ink-3 group-hover:text-ink-1'
              }`}
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              strokeWidth={2}
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2"
              />
            </svg>
          ),
        },
        {
          id: 'snippets',
          label: 'Snippets',
          icon: (active) => (
            <svg
              className={`h-4 w-4 transition-colors duration-200 ${
                active ? 'text-emerald-300' : 'text-ink-3 group-hover:text-ink-1'
              }`}
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              strokeWidth={2}
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z"
              />
            </svg>
          ),
        },
        {
          id: 'notes',
          label: 'Quick Notes',
          icon: (active) => (
            <svg
              className={`h-4 w-4 transition-colors duration-200 ${
                active ? 'text-amber-300' : 'text-ink-3 group-hover:text-ink-1'
              }`}
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              strokeWidth={2}
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z"
              />
            </svg>
          ),
        },
        {
          id: 'emoji-picker',
          label: 'Emoji Picker',
          icon: (active) => (
            <svg
              className={`h-4 w-4 transition-colors duration-200 ${
                active ? 'text-yellow-300' : 'text-ink-3 group-hover:text-ink-1'
              }`}
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              strokeWidth={2}
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="M14.828 14.828a4 4 0 01-5.656 0M9 10h.01M15 10h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
              />
            </svg>
          ),
        },
      ],
    },
    {
      title: 'System',
      items: [
        {
          id: 'open-ports',
          label: 'Open Ports',
          icon: (active) => (
            <svg
              className={`h-4 w-4 transition-colors duration-200 ${
                active ? 'text-cyan-300' : 'text-ink-3 group-hover:text-ink-1'
              }`}
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              strokeWidth={2}
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="M5.636 18.364a9 9 0 010-12.728m12.728 0a9 9 0 010 12.728m-9.9-2.829a5 5 0 010-7.07m7.072 0a5 5 0 010 7.07M13 12a1 1 0 11-2 0 1 1 0 012 0z"
              />
            </svg>
          ),
        },
        {
          id: 'terminal',
          label: 'Terminal',
          icon: (active) => (
            <svg
              className={`h-4 w-4 transition-colors duration-200 ${
                active ? 'text-teal-300' : 'text-ink-3 group-hover:text-ink-1'
              }`}
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              strokeWidth={2}
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="M12 12a1 1 0 001-1V8.558a1 1 0 00-1-1H9M4 18.4V5.6A1.6 1.6 0 015.6 4h12.8A1.6 1.6 0 0120 5.6v12.8a1.6 1.6 0 01-1.6 1.6H5.6A1.6 1.6 0 014 18.4z"
              />
            </svg>
          ),
        },
        {
          id: 'extensions',
          label: 'Extensions',
          icon: (active) => (
            <svg
              className={`h-4 w-4 transition-colors duration-200 ${
                active ? 'text-rose-300' : 'text-ink-3 group-hover:text-ink-1'
              }`}
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              strokeWidth={2}
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="M11 4a2 2 0 114 0v1a1 1 0 001 1h3a1 1 0 011 1v3a1 1 0 01-1 1h-1a2 2 0 100 4h1a1 1 0 011 1v3a1 1 0 01-1 1h-3a1 1 0 01-1-1v-1a2 2 0 10-4 0v1a1 1 0 01-1 1H7a1 1 0 01-1-1v-3a1 1 0 00-1-1H4a1 1 0 01-1-1v-3a1 1 0 011-1h1a2 2 0 100-4H4a1 1 0 01-1-1V7a1 1 0 011-1h3a1 1 0 001-1V4z"
              />
            </svg>
          ),
        },
      ],
    },
    {
      title: 'Config',
      items: [
        {
          id: 'settings',
          label: 'Settings',
          icon: (active) => (
            <svg
              className={`h-4 w-4 transition-colors duration-200 ${
                active ? 'text-slate-300' : 'text-ink-3 group-hover:text-ink-1'
              }`}
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              strokeWidth={2}
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z"
              />
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"
              />
            </svg>
          ),
        },
      ],
    },
  ]

  return (
    <aside className="cleanmymac-sidebar flex h-full w-[210px] shrink-0 flex-col border-r border-white/[0.06] bg-[#0c0d12]/45 p-4 backdrop-blur-xl no-drag">
      {/* macOS Window Controls */}
      <div
        className="mb-6 flex items-center gap-2 px-1"
        onMouseEnter={() => setHoveredDot(true)}
        onMouseLeave={() => setHoveredDot(false)}
      >
        <button
          onClick={handleClose}
          className="group relative flex h-3 w-3 items-center justify-center rounded-full bg-[#ff5f56] transition active:brightness-75"
          aria-label="Close window"
        >
          {hoveredDot && (
            <svg className="h-1.5 w-1.5 text-red-950" viewBox="0 0 12 12" fill="currentColor">
              <path d="M1.3,1.3 L10.7,10.7 M10.7,1.3 L1.3,10.7" stroke="currentColor" strokeWidth="2" />
            </svg>
          )}
        </button>
        <button
          onClick={handleMinimize}
          className="group relative flex h-3 w-3 items-center justify-center rounded-full bg-[#ffbd2e] transition active:brightness-75"
          aria-label="Minimize window"
        >
          {hoveredDot && (
            <svg className="h-1.5 w-1.5 text-amber-950" viewBox="0 0 12 12" fill="currentColor">
              <rect x="1" y="5" width="10" height="2" />
            </svg>
          )}
        </button>
        <button
          className="group relative flex h-3 w-3 items-center justify-center rounded-full bg-[#27c93f] opacity-60 cursor-not-allowed"
          aria-label="Maximize window"
          disabled
        >
          {hoveredDot && (
            <svg className="h-1.5 w-1.5 text-emerald-950" viewBox="0 0 12 12" fill="currentColor">
              <path d="M0,0 L0,5 L5,5 L5,0 L0,0 Z M7,7 L7,12 L12,12 L12,7 L7,7 Z" />
            </svg>
          )}
        </button>
      </div>

      {/* Navigation Groups */}
      <div className="flex-1 space-y-5 overflow-y-auto pr-1 select-none">
        {categories.map((category) => (
          <div key={category.title} className="space-y-1">
            <h3 className="px-2 text-[9.5px] font-bold uppercase tracking-[0.14em] text-ink-4">
              {category.title}
            </h3>
            <ul className="space-y-0.5">
              {category.items.map((item) => {
                const active = activeSurface === item.id
                return (
                  <li key={item.id}>
                    <button
                      onClick={() => onSelectSurface(item.id)}
                      className={`group relative flex w-full items-center gap-3 rounded-lg px-2.5 py-1.5 text-left transition-all duration-200 ${
                        active
                          ? 'bg-white/[0.07] text-white shadow-[inset_0_1px_0_rgba(255,255,255,0.05)]'
                          : 'text-ink-3 hover:bg-white/[0.03] hover:text-ink-1'
                      }`}
                    >
                      {/* Selected side highlight indicator */}
                      {active && (
                        <span className="absolute left-0 top-1.5 bottom-1.5 w-[3px] rounded-r-full bg-gradient-to-b from-indigo-400 to-purple-400 shadow-[0_0_8px_rgba(139,141,247,0.8)]" />
                      )}
                      <span className="flex h-5 w-5 items-center justify-center">
                        {item.icon(active)}
                      </span>
                      <span className="text-[12.5px] font-medium tracking-tight">
                        {item.label}
                      </span>
                    </button>
                  </li>
                )
              })}
            </ul>
          </div>
        ))}
      </div>
    </aside>
  )
}
