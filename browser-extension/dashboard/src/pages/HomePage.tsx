import { Link } from "react-router-dom";
import { useExtension } from "../sdk-context";
import "./HomePage.css";

export function HomePage() {
  const { catalog, managed, status } = useExtension();
  return (
    <div className="home">
      <section className="hero card">
        <div className="hero-text">
          <h1>의도와 실행 사이의 무결성</h1>
          <p>
            Scopeball은 Cedar 정책으로 모든 트랜잭션을 사전 검증합니다. 정책을
            작성하고, Policy Test로 미리보고, 메타마스크 서명 직전 자동
            게이팅합니다.
          </p>
          <div className="hero-actions">
            <Link to="/editor" className="btn btn-primary">
              새 정책 작성
            </Link>
            <Link to="/library" className="btn btn-secondary">
              내 정책 보기
            </Link>
          </div>
        </div>
      </section>

      <section className="stats">
        <StatCard
          label="알려진 정책"
          value={catalog ? catalog.policies.length : "—"}
          hint="기본 + 마켓플레이스 + 대시보드 등록 합계"
        />
        <StatCard
          label="활성화"
          value={catalog ? catalog.enabled.length : "—"}
          hint="현재 평가 대상"
        />
        <StatCard
          label="내가 만든 정책"
          value={managed ? managed.length : "—"}
          hint="이 대시보드에서 등록한 정책만"
          tone="green"
        />
      </section>

      <section className="surfaces">
        <SurfaceCard
          title="Editor"
          subtitle="Builder · Code 모드로 정책 작성"
          to="/editor"
        />
        <SurfaceCard
          title="Library"
          subtitle="내가 만든 정책 활성화 · 삭제"
          to="/library"
        />
        <SurfaceCard
          title="Audit"
          subtitle="Pass / Warn / Fail 히스토리 (예정)"
          to="/audit"
        />
        <SurfaceCard
          title="Settings"
          subtitle="네트워크 · 옵션"
          to="/settings"
        />
      </section>

      {status.kind === "error" ? (
        <section className="banner-err card">
          <strong>Extension 연결 실패</strong>
          <span>{status.message}</span>
          <span className="hint">
            확장프로그램을 unpacked로 로드하고 활성화되어 있는지 확인해주세요.
          </span>
        </section>
      ) : null}
    </div>
  );
}

function StatCard({
  label,
  value,
  hint,
  tone = "default",
}: {
  label: string;
  value: number | string;
  hint: string;
  tone?: "default" | "green";
}) {
  return (
    <div className={`stat-card card ${tone === "green" ? "tone-green" : ""}`}>
      <div className="stat-label">{label}</div>
      <div className="stat-value">{value}</div>
      <div className="stat-hint">{hint}</div>
    </div>
  );
}

function SurfaceCard({
  title,
  subtitle,
  to,
}: {
  title: string;
  subtitle: string;
  to: string;
}) {
  return (
    <Link to={to} className="surface-card">
      <div className="surface-title">{title}</div>
      <div className="surface-sub">{subtitle}</div>
    </Link>
  );
}
