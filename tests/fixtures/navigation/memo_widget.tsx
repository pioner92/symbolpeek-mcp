type ComponentType = () => null;

function memo(component: ComponentType): ComponentType {
  return component;
}

function WidgetComponent(): null {
  return null;
}

export const Widget = memo(WidgetComponent);

export function Screen() {
  return <Widget />;
}
