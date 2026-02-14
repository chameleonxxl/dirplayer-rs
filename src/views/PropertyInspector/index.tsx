import { JSONTree } from "react-json-tree";
import styles from "./styles.module.css";
import { TSelectedObject } from "../../store/uiSlice";
import { useSelectedObjects } from "../../hooks/selection";
import { useCallback, useMemo } from "react";
import { Layout, Model, TabNode } from "flexlayout-react";
import MovieChunksView from "./MovieChunksView";

interface PropertyInspectorProps {
  selectedObject?: TSelectedObject;
}

export default function PropertyInspector({
  selectedObject,
}: PropertyInspectorProps) {
  const { scoreBehaviorRef, selectedSprite, member, secondaryMember } = useSelectedObjects();

  const model = useMemo(() => Model.fromJson({
    global: {
      tabEnableClose: false,
    },
    layout: {
      type: "row",
      children: [
        {
          type: "tabset",
          children: [
            ...(scoreBehaviorRef ? [{
              type: "tab",
              name: "Score Behavior",
              component: "scoreBehavior"
            }] : []),
            ...(selectedObject?.type === "sprite" ? [{
              type: "tab",
              name: "Sprite",
              component: "sprite"
            }] : []),
            ...(member ? [{
              type: "tab",
              name: "Member",
              component: "member"
            }] : []),
            ...(secondaryMember ? [{
              type: "tab",
              name: "Secondary Member",
              component: "secondaryMember"
            }] : []),
            {
              type: "tab",
              name: "Movie",
              component: "movie"
            }
          ]
        }
      ]
    }
  }), [scoreBehaviorRef, selectedObject, member, secondaryMember]);

  const factory = useCallback((node: TabNode) => {
    switch (node.getComponent()) {
      case "scoreBehavior":
        return <JSONTree keyPath={["scoreBehavior"]} data={scoreBehaviorRef} />;
      case "sprite":
        return <JSONTree keyPath={["sprite"]} data={{ ...selectedSprite }} />;
      case "member":
        return <JSONTree keyPath={["member"]} data={member} />;
      case "secondaryMember":
        return <JSONTree keyPath={["secondaryMember"]} data={secondaryMember} />;
      case "movie":
        return <MovieChunksView />;
      default:
        return null;
    }
  }, [scoreBehaviorRef, selectedSprite, member, secondaryMember]);

  return <div className={styles.container}>
    <Layout model={model} factory={factory} />
  </div>;
}
