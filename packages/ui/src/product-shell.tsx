type ProductShellProps = {
  eyebrow: string;
  title: string;
  description: string;
};

export function ProductShell({ eyebrow, title, description }: ProductShellProps) {
  return (
    <main
      style={{
        minHeight: "100vh",
        display: "grid",
        placeItems: "center",
        padding: "48px 24px"
      }}
    >
      <section style={{ width: "min(920px, 100%)" }}>
        <p
          style={{
            margin: "0 0 16px",
            fontSize: 14,
            fontWeight: 700,
            letterSpacing: 0,
            textTransform: "uppercase",
            color: "#1570ef"
          }}
        >
          {eyebrow}
        </p>
        <h1
          style={{
            margin: 0,
            fontSize: 56,
            lineHeight: 1,
            letterSpacing: 0,
            color: "#101828"
          }}
        >
          {title}
        </h1>
        <p
          style={{
            margin: "24px 0 0",
            maxWidth: 680,
            fontSize: 20,
            lineHeight: 1.5,
            color: "#475467"
          }}
        >
          {description}
        </p>
      </section>
    </main>
  );
}
