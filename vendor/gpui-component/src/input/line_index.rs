//! Indexed sequence of source lines. Implicit treap with cached subtree heights.
use super::text_wrapper::LineItem;
use std::ops::{Index, RangeInclusive};
type Link = Option<Box<Node>>;
struct Node {
    item: LineItem,
    left: Link,
    right: Link,
    priority: u64,
    count: usize,
    height: f32,
    wraps: usize,
}
fn count(n: &Link) -> usize {
    n.as_ref().map_or(0, |n| n.count)
}
fn height(n: &Link) -> f32 {
    n.as_ref().map_or(0., |n| n.height)
}
fn wraps(n: &Link) -> usize {
    n.as_ref().map_or(0, |n| n.wraps)
}
impl Node {
    fn refresh(&mut self) {
        self.count = 1 + count(&self.left) + count(&self.right);
        self.height = height(&self.left)
            + height(&self.right)
            + self.item.lines_len() as f32 * self.item.scale;
        self.wraps = wraps(&self.left) + wraps(&self.right) + self.item.lines_len();
    }
}
fn split(tree: Link, at: usize) -> (Link, Link) {
    let Some(mut n) = tree else {
        return (None, None);
    };
    if at <= count(&n.left) {
        let (a, b) = split(n.left.take(), at);
        n.left = b;
        n.refresh();
        (a, Some(n))
    } else {
        let left = count(&n.left);
        let (a, b) = split(n.right.take(), at - left - 1);
        n.right = a;
        n.refresh();
        (Some(n), b)
    }
}
fn merge(a: Link, b: Link) -> Link {
    match (a, b) {
        (None, b) => b,
        (a, None) => a,
        (Some(mut a), Some(mut b)) => {
            if a.priority > b.priority {
                a.right = merge(a.right.take(), Some(b));
                a.refresh();
                Some(a)
            } else {
                b.left = merge(Some(a), b.left.take());
                b.refresh();
                Some(b)
            }
        }
    }
}
#[derive(Default)]
pub(super) struct Lines {
    root: Link,
    serial: u64,
}
impl Lines {
    pub fn len(&self) -> usize {
        count(&self.root)
    }
    pub fn total_height(&self) -> f32 {
        height(&self.root)
    }
    pub fn wraps(&self) -> usize {
        wraps(&self.root)
    }
    pub fn prefix(&self, mut row: usize) -> f32 {
        let mut n = self.root.as_deref();
        let mut sum = 0.;
        while let Some(node) = n {
            let left = count(&node.left);
            if row <= left {
                n = node.left.as_deref();
            } else {
                sum += height(&node.left) + node.item.lines_len() as f32 * node.item.scale;
                row -= left + 1;
                n = node.right.as_deref();
            }
        }
        sum
    }
    pub fn row_at(&self, mut y: f32) -> usize {
        let mut n = self.root.as_deref();
        let mut row = 0;
        while let Some(node) = n {
            let h = height(&node.left);
            if y < h {
                n = node.left.as_deref();
            } else if y < h + node.item.lines_len() as f32 * node.item.scale {
                return row + count(&node.left);
            } else {
                y -= h + node.item.lines_len() as f32 * node.item.scale;
                row += count(&node.left) + 1;
                n = node.right.as_deref();
            }
        }
        row.min(self.len().saturating_sub(1))
    }
    pub fn prefix_wraps(&self, mut row: usize) -> usize {
        let mut n = self.root.as_deref();
        let mut sum = 0;
        while let Some(node) = n {
            let left = count(&node.left);
            if row <= left {
                n = node.left.as_deref();
            } else {
                sum += wraps(&node.left) + node.item.lines_len();
                row -= left + 1;
                n = node.right.as_deref();
            }
        }
        sum
    }
    pub fn row_at_wrap(&self, mut y: usize) -> usize {
        let mut n = self.root.as_deref();
        let mut row = 0;
        while let Some(node) = n {
            let h = wraps(&node.left);
            if y < h {
                n = node.left.as_deref();
            } else if y < h + node.item.lines_len() {
                return row + count(&node.left);
            } else {
                y -= h + node.item.lines_len();
                row += count(&node.left) + 1;
                n = node.right.as_deref();
            }
        }
        row.min(self.len().saturating_sub(1))
    }
    pub fn get(&self, mut row: usize) -> Option<&LineItem> {
        let mut n = self.root.as_deref();
        while let Some(node) = n {
            let left = count(&node.left);
            if row < left {
                n = node.left.as_deref();
            } else if row == left {
                return Some(&node.item);
            } else {
                row -= left + 1;
                n = node.right.as_deref();
            }
        }
        None
    }
    pub fn iter(&self) -> impl Iterator<Item = &LineItem> {
        (0..self.len()).map(|i| &self[i])
    }
    pub fn splice(&mut self, range: RangeInclusive<usize>, items: Vec<LineItem>) {
        let (a, rest) = split(self.root.take(), *range.start());
        let (_, b) = split(rest, range.end() - range.start() + 1);
        let mut middle = None;
        for item in items {
            self.serial += 1;
            let mut p = self.serial.wrapping_mul(0x9e3779b97f4a7c15);
            p = (p ^ (p >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
            p = (p ^ (p >> 27)).wrapping_mul(0x94d049bb133111eb);
            p ^= p >> 31;
            let mut node = Box::new(Node {
                item,
                left: None,
                right: None,
                priority: p,
                count: 1,
                height: 0.,
                wraps: 0,
            });
            node.refresh();
            middle = merge(middle, Some(node));
        }
        self.root = merge(merge(a, middle), b);
    }
}
impl Index<usize> for Lines {
    type Output = LineItem;
    fn index(&self, i: usize) -> &LineItem {
        self.get(i).expect("line index")
    }
}
impl From<Vec<LineItem>> for Lines {
    fn from(items: Vec<LineItem>) -> Self {
        let mut s = Self::default();
        s.splice(0..=0, items);
        s
    }
}

#[cfg(test)]
mod index_tests {
    use super::*;
    use std::rc::Rc;
    fn item(n: usize) -> LineItem {
        LineItem::diagnostic_line(n)
    }
    #[test]
    fn indexed_insert_remove_lookup() {
        let mut model = (0..100_000).map(|i| i % 4 + 1).collect::<Vec<_>>();
        let mut lines: Lines = model.iter().map(|&n| item(n)).collect::<Vec<_>>().into();
        for (at, removed, replacement) in [
            (0, 1, vec![2, 3]),
            (50_000, 3, vec![1]),
            (99_998, 1, vec![4, 4]),
        ] {
            lines.splice(
                at..=at + removed - 1,
                replacement.iter().map(|&n| item(n)).collect(),
            );
            model.splice(at..at + removed, replacement);
            assert_eq!(lines.len(), model.len());
            assert_eq!(lines.wraps(), model.iter().sum::<usize>());
            for row in [0, at, model.len() - 1] {
                let height = model[..row].iter().sum::<usize>();
                assert_eq!(lines.prefix(row), height as f32);
                assert_eq!(lines.row_at(height as f32), row);
                assert_eq!(lines.row_at_wrap(height), row);
            }
        }
        let _ = Rc::new(());
    }
}
