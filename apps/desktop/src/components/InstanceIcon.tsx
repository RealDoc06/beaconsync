import { useQuery } from '@tanstack/react-query'
import { call } from '../lib/api'

/** The instance's Prism icon. Keeps its box while loading or when there is none. */
export function InstanceIcon({ path, size }: { path: string; size: number }) {
  const icon = useQuery({ queryKey: ['icon', path], queryFn: () => call('instance_icon', { path }), staleTime: Infinity })
  return (
    <span className="shrink-0" style={{ width: size, height: size }}>
      {icon.data && <img src={icon.data} alt="" width={size} height={size} className="size-full object-contain [image-rendering:pixelated]" />}
    </span>
  )
}
