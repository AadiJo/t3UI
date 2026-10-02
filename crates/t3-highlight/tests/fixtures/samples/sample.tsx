import { useState } from "react";

export function Counter({ initial = 0 }: { initial?: number }) {
  const [count, setCount] = useState(initial);
  return (
    <button className="rounded-md px-2" onClick={() => setCount((value) => value + 1)}>
      Clicked {count} times
    </button>
  );
}
