class QueueItem {
  final String name;
  final String time;
  final double pct;
  final bool active;

  const QueueItem({
    required this.name,
    required this.time,
    required this.pct,
    required this.active,
  });

  QueueItem withPct(double newPct) => QueueItem(
        name: name,
        time: time,
        pct: newPct.clamp(0, 100),
        active: active,
      );
}

class ResearchState {
  final String name;
  final String time;
  final double pct;
  final String fragments;
  final List<String> completed;

  const ResearchState({
    required this.name,
    required this.time,
    required this.pct,
    required this.fragments,
    required this.completed,
  });

  ResearchState withPct(double newPct) => ResearchState(
        name: name,
        time: time,
        pct: newPct.clamp(0, 100),
        fragments: fragments,
        completed: completed,
      );
}
