/** A green tick for something in place, named for screen readers. */
function Check(props: { label: string }) {
  return (
    <span class="check" role="img" aria-label={props.label}>
      ✓
    </span>
  );
}

export default Check;
