import { useId } from 'react'

interface Props {
  size?: number
  className?: string
  style?: React.CSSProperties
}

export default function RadarIcon({ size = 32, className, style }: Props) {
  const uid = useId().replace(/:/g, '')
  const bgId = `ri-bg-${uid}`

  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 512 512"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      className={className}
      style={style}
    >
      <defs>
        <linearGradient id={bgId} x1="0" y1="512" x2="512" y2="0" gradientUnits="userSpaceOnUse">
          <stop stopColor="#1A0A6B" />
          <stop offset="1" stopColor="#4515F0" />
        </linearGradient>
      </defs>

      <rect width="512" height="512" rx="112" fill={`url(#${bgId})`} />

      {/* Crosshairs */}
      <line x1="112" y1="256" x2="400" y2="256" stroke="white" strokeOpacity="0.06" strokeWidth="1" />
      <line x1="256" y1="112" x2="256" y2="400" stroke="white" strokeOpacity="0.06" strokeWidth="1" />

      {/* Rings */}
      <circle cx="256" cy="256" r="88"  stroke="white" strokeOpacity="0.14" strokeWidth="1.5" strokeDasharray="6 10" />
      <circle cx="256" cy="256" r="148" stroke="white" strokeOpacity="0.09" strokeWidth="1.5" strokeDasharray="6 10" />

      {/* Sweep group centred at icon origin */}
      <g transform="translate(256,256)">
        {/* Trailing sector 12→1 o'clock */}
        <path d="M 0 0 L 0 -148 A 148 148 0 0 1 74 -128.1 Z" fill="#5B33F0" fillOpacity="0.18" />
        {/* Arm */}
        <line x1="0" y1="0" x2="74" y2="-128.1" stroke="white" strokeOpacity="0.75" strokeWidth="2.5" strokeLinecap="round" />
        {/* Centre dot */}
        <circle cx="0"  cy="0"      r="5"  fill="white"   fillOpacity="0.45" />
        {/* Neon ping */}
        <circle cx="74" cy="-128.1" r="26" fill="#B3FC4F" fillOpacity="0.18" />
        <circle cx="74" cy="-128.1" r="17" fill="#B3FC4F" fillOpacity="0.72" />
        <circle cx="74" cy="-128.1" r="9"  fill="#EEFFAA" />
      </g>
    </svg>
  )
}
