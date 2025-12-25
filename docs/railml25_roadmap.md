# railML 2.5 지원 로드맵

## 목적
railML 2.5 스키마를 완전히 임포트/익스포트하고 Junction 모델에 정확히 반영.

## 단계별 계획 (각 단계 완료 시 확인 요청)

1) 파싱 확장
- `lib/railmlio/src/model.rs`에 2.5 필드/객체 추가: 트랙 속성(type/mainDir), trackElements(플랫폼, 속도제한, 건널목 등), ocsElements(검지기/보호/디레일러/발리스), metadata, OCP, trackGroups/lines, states.
- `xml.rs` 파서 확장. 샘플 2.3/2.4/2.5 파일로 필수 요소 개수/속성 단위 테스트 추가.
- 완료 기준: 샘플 파서 테스트 통과, 누락 필드 없음.

2) 토폴로지 견고화
- `topo.rs`에서 다중 switch connection, crossing, continuation, macroscopic node 처리 및 id/이름 보존.
- 모든 connection 해소, 노드/엣지 수 기대치 검증 테스트.
- 완료 기준: 2.5 샘플에서 미해결 connection 0, 테스트 통과.

3) 마일리지/지오메트리
- `src/import.rs` `MileageMethod::FromFile` 구현: `pos`/`absPos`, 트랙 길이 활용; Estimated는 폴백.
- absPos 단조/시작·끝 오프셋 비교 테스트 추가.
- 완료 기준: 2.5 샘플에서 FromFile 모드 테스트 통과, 레이아웃 왜곡 이슈 없음.

4) 오브젝트 매핑
- `RailObject`/`convert_railplot`/`convert_junction` 확장: 신호(기능/타입), 검지기/발리스/열차보호, 디레일러, 건널목, 속도변경 마커, 플랫폼 등 Junction 오브젝트화.
- 오브젝트 유형/개수 단언 테스트.
- 완료 기준: 2.5 샘플에서 기대 오브젝트 모두 생성, 테스트 통과.

5) UI/UX
- 임포트 UI 문구 갱신, 마일리지 모드 선택 옵션(필요 시), 에러 메시지 개선.
- 수동 임포트 확인 및 사용자 플로우 점검.
- 완료 기준: UI 통해 2.5 파일 임포트 성공, 오류 시 친절한 메시지 표시.

6) 익스포트
- `lib/railmlio`에 railML 2.5 writer 설계/구현, GUI “Export to railML…” 연결.
- 임포트→익스포트→재임포트 라운드트립 테스트로 토폴로지/오브젝트 보존 확인.
- 완료 기준: 라운드트립 테스트 통과.

7) 회귀/성능
- 2.3/2.4/2.5 회귀 스위트 및 소형 2.5 픽스처(다중 스위치/크로싱/검지기/absPos 간격).
- schematic solver가 에러 없이 비어 있지 않은 모델 반환하는지 테스트.
- 완료 기준: 회귀/솔버 테스트 모두 통과.

## 진행 방식
- 각 단계 완료 시 테스트/결과 공유 → 확인 후 다음 단계 착수.
- 실패 시 원인/대책 제시 후 재시도.
