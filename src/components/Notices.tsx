import { useStore } from "../store/session";

export default function Notices() {
  const notices = useStore((s) => s.notices);
  const dismiss = useStore((s) => s.dismiss);
  if (notices.length === 0) return null;

  return (
    <>
      {notices.map((n) => (
        <div key={n.id} className={`banner ${n.kind}`}>
          <div>
            <strong>{n.title}</strong>
            {n.detail && <div>{n.detail}</div>}
            {n.steps && (
              <ol>
                {n.steps.map((s, i) => (
                  <li key={i}>{s}</li>
                ))}
              </ol>
            )}
          </div>
          <button className="close" onClick={() => dismiss(n.id)} title="Dismiss">
            ✕
          </button>
        </div>
      ))}
    </>
  );
}
