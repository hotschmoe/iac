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
  final List<String> completed;

  /// Projects waiting behind the running one.
  final List<QueueItem> waiting;

  const ResearchState({
    required this.name,
    required this.time,
    required this.pct,
    required this.completed,
    this.waiting = const [],
  });

  ResearchState withPct(double newPct) => ResearchState(
        name: name,
        time: time,
        pct: newPct.clamp(0, 100),
        completed: completed,
        waiting: waiting,
      );
}
