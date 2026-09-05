import { TreeStructure } from "@phosphor-icons/react";

import { Card, EmptyState } from "../components/ui";
import type { Store } from "../lib/store";

export function Rules({ store }: { store: Store }) {
  const { t } = store;
  return (
    <div className="stack">
      <Card title={t("rules.title")} subtitle={t("rules.body")}>
        <EmptyState
          icon={<TreeStructure size={26} />}
          title={t("rules.soon")}
          body={t("rules.soonBody")}
        />
      </Card>
    </div>
  );
}
