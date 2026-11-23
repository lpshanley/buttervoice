import ghee from "./assets/ghee.png";

/** Ghee in full colour, for the app's own windows. The outlined face is
 * for the menu bar. */
function GheeMascot(props: { size: number }) {
  return <img src={ghee} width={props.size} height={Math.round(props.size * 0.95)} alt="" />;
}

export default GheeMascot;
