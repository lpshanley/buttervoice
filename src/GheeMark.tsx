import { PATHS, STROKE } from "./ghee";

/** Ghee's outlined face, drawn in the current text colour. */
function GheeMark(props: { size: number }) {
  return (
    <svg
      width={props.size}
      height={props.size}
      viewBox="0 0 22 22"
      fill="currentColor"
      stroke="currentColor"
      stroke-width={STROKE}
      stroke-linejoin="round"
      aria-hidden="true"
    >
      <path fill-rule="evenodd" d={PATHS.outline} />
      <path d={PATHS.leftEye} />
      <path d={PATHS.rightEye} />
      <path d={PATHS.smile} />
    </svg>
  );
}

export default GheeMark;
