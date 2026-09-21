import type { SVGProps } from 'react';
import { siFujifilm } from 'simple-icons';

type FujiFilmIconProps = SVGProps<SVGSVGElement> & {
  size?: number | string;
};

/** Official Fujifilm wordmark via simple-icons (CC0), themed with currentColor. */
export default function FujiFilmIcon({ size = 20, className, ...props }: FujiFilmIconProps) {
  return (
    <svg
      role="img"
      viewBox="0 0 24 24"
      width={size}
      height={size}
      className={className}
      fill="currentColor"
      xmlns="http://www.w3.org/2000/svg"
      aria-hidden={props['aria-label'] ? undefined : true}
      {...props}
    >
      <title>Fujifilm</title>
      <path d={siFujifilm.path} />
    </svg>
  );
}
