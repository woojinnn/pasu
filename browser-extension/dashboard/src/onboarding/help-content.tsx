import type { ReactNode } from "react";

export interface TourStep {
  title: string;
  body: ReactNode;
}

export const TOUR_STEPS: TourStep[] = [
  {
    title: "Scopeball은 Cedar 정책 게이트입니다",
    body: (
      <>
        모든 트랜잭션이 메타마스크 서명 직전 <strong>Cedar 정책</strong>으로
        사전 검증됩니다. Pass면 통과, Warn은 경고창, Fail은 차단됩니다. 이
        대시보드에서 정책을 만들고 관리하세요.
      </>
    ),
  },
  {
    title: "Editor — Builder / Code 두 가지 방법",
    body: (
      <>
        <strong>Builder</strong>는 폼으로 필드·조건을 채워서 Cedar를
        자동 생성합니다. <strong>Code</strong>는 Cedar 텍스트를 직접 작성합니다.
        Code 직접 편집은 경고 모달을 거쳐야 진입할 수 있으며, 이후
        Builder로 복원되지 않습니다. Policy Test 패널에서 즉시 verdict를
        확인하세요.
      </>
    ),
  },
  {
    title: "Library — 활성화 · 정렬 · 내보내기",
    body: (
      <>
        등록한 정책은 <strong>Library</strong>에서 일괄 관리합니다. 검색,
        토글, JSON export/import로 백업·공유가 가능하며, "Editor에서 열기"로
        편집 진입 가능합니다.
      </>
    ),
  },
];

export interface HelpEntry {
  title: string;
  body: ReactNode;
}

export const HELP_ENTRIES: HelpEntry[] = [
  {
    title: "정책 ID 규칙",
    body: (
      <>
        대시보드에서 만드는 모든 정책 ID는 <code>dashboard::</code> 로
        시작해야 합니다. 본문 32 KiB 이하, 최대 200개까지 저장 가능합니다.
      </>
    ),
  },
  {
    title: "Policy Test = 평가 미리보기",
    body: (
      <>
        EVM 시뮬레이션이 아닙니다. 가상의 raw_request로 정책 엔진을 한 번
        돌려 <strong>어떤 verdict가 나올지</strong> 미리 확인할 뿐입니다.
        가스 추정·잔액 변화는 측정되지 않습니다.
      </>
    ),
  },
  {
    title: "Code → Builder 역변환",
    body: (
      <>
        Builder에서 만든 정책은 <code>parse_cedar</code> 로 복원 가능하지만,
        Code에서 직접 손본 정책은 PolicyRule 부분집합에서 벗어나면 Builder로
        역변환되지 않습니다. 이 경우 Library에서 "Editor 열기" 시 Code
        모드로 들어옵니다.
      </>
    ),
  },
  {
    title: "Settings 자동 저장",
    body: (
      <>
        Settings의 값은 localStorage에 즉시 저장됩니다. Policy Test 기본값과
        자동 새로고침 토글만 영향을 받으며, 다른 도메인·기기와는 동기화되지
        않습니다.
      </>
    ),
  },
];
