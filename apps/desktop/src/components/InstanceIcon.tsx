import { useQuery } from '@tanstack/react-query'
import { call } from '../lib/api'
import { CubeIcon } from './Icons'

/** The instance's Prism icon inside a subtle framed tile. Keeps its box while loading or when none is set. */
export function InstanceIcon({ path, size, framed = true }: { path: string; size: number; framed?: boolean }) {
  const icon = useQuery({ queryKey: ['icon', path], queryFn: () => call('instance_icon', { path }), staleTime: Infinity })
  return (
    <span
      className={`grid shrink-0 place-items-center overflow-hidden ${
        framed ? 'rounded-lg border border-line bg-surface-2/80 shadow-theme-xs' : ''
      }`}
      style={{ width: size, height: size }}
    >
      {icon.data ? (
        <img
          src={icon.data}
          alt=""
          width={size}
          height={size}
          className={`${framed ? 'size-[78%]' : 'size-full'} object-contain [image-rendering:pixelated]`}
        />
      ) : (
        <CubeIcon className="text-subtle" style={{ width: Math.round(size * 0.52), height: Math.round(size * 0.52) }} />
      )}
    </span>
  )
}
