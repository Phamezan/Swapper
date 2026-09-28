/** Swapper's app icon (public/swapper.svg), drawn inline so the desktop and
 *  phone headers match the tray, taskbar and favicon. */
export function BrandIcon({ size = 35, className }: { size?: number; className?: string }) {
  return (
    <svg className={className} width={size} height={size} viewBox="0 0 512 512" aria-hidden focusable="false">
      <rect x="10" y="10" width="492" height="492" rx="114" fill="#141a1b" />
      <rect x="16" y="16" width="480" height="480" rx="108" fill="none" stroke="#738d67" strokeWidth="12" />
      <path d="M126 207h235l-54-54" fill="none" stroke="#c5e39e" strokeWidth="42" strokeLinecap="round" strokeLinejoin="round" />
      <path d="M386 305H151l54 54" fill="none" stroke="#c5e39e" strokeWidth="42" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}
