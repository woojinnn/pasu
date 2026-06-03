# compliance/ — 규제·제재·관할 mandate 정책 (예약 슬롯)

precedence 1위 버킷. **현재 0 set** (예약).

이 버킷에는 보안 best-practice 가 아니라 **규제 / 제재 / 관할 의무(mandate)** 를 인코딩한 정책만 둔다:

- **OFAC sanctions** — SDN / Consolidated / Digital Currency Address 차단. feed = OFAC Sanctions List Service.
- **FATF Travel Rule** — 임계 초과 transfer 의 보고/확인 (예: $3,000 / $1,000 jurisdiction별).
- **Jurisdiction 제한** — 특정 관할에서 금지된 자산/행위.

## precedence 상 위치

규칙 `compliance > protocol > wallet > action`, first-match-wins. 같은 정책이 wallet/action 성격을 **겸해도 규제 mandate 이면 여기로** 온다. 판별 기준은 **list/threshold 의 source**:

- 단순 user/operator denylist → `wallet/recipient-denylist`
- 같은 차단인데 **list 가 OFAC SDN feed** → `compliance/`
- $10k user spend cap → `wallet/usd-cap`
- 같은 임계인데 **travel-rule 보고 의무** → `compliance/`

현 카탈로그의 `transfer-recipient-denylist`, `transfer-recipient-reputation`, `transfer-usd-cap` 는
`tag:could-be-compliance-*` 로 표시되어 있다 — feed/근거가 규제로 바뀌면 이 버킷으로 승격 후보.

## 첫 규제정책 추가 시

```
compliance/<sub>/<id>/
├─ policy.cedar      # 규제 근거를 cedar 주석 1차출처로 (OFAC/FATF 문서)
└─ manifest.json     # 보통 enrichment (sanctions feed 조회)
_methods/<m>.md      # 예: compliance.sanctions_screen → {listed:Bool, list:String, ofacId:String}
```

신규 메서드는 `_methods/<m>.md` 작성 + (구현 시) `schema/method-catalog.json` 등록.
authoring 절차는 루트 `README.md` 의 체크리스트 참조.
